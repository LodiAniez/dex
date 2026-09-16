//! `dex context …`: the shared store, from the command line (docs/prd.md §11).
//!
//! Agents normally reach this through the MCP tools; these commands are for the
//! human, and for debugging what an agent would be told.

use clap::Subcommand;
use dex_protocol::ErrorBody;
use dex_protocol::context::{
    Appended, Digest, EntryList, EntryView, Inbox, SearchResults, Written,
};
use serde_json::{Value, json};

use crate::output::{self, Format};
use dex_cli::client;

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Print one entry's value.
    Read {
        /// Its key.
        key: String,
    },
    /// Store a fact under a key.
    Write {
        /// Its key: lowercase, `/` for namespaces.
        key: String,
        /// Its value.
        value: String,
        /// Comma-separated tags.
        #[arg(long)]
        tags: Option<String>,
        /// Fail unless the stored version matches; `0` means create only.
        #[arg(long)]
        expected_version: Option<i64>,
    },
    /// List keys and metadata, without values.
    List {
        /// Only entries carrying this tag.
        #[arg(long)]
        tag: Option<String>,
    },
    /// Append a freeform line to the workspace log.
    Note {
        /// What to record; words are joined with spaces.
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        body: Vec<String>,
        /// Comma-separated tags.
        #[arg(long)]
        tags: Option<String>,
    },
    /// Send a directed message to another agent.
    Send {
        /// Agent id or label, or its pane's label.
        target: String,
        /// The message; words are joined with spaces.
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        body: Vec<String>,
    },
    /// Read and clear this pane's unread messages.
    Inbox,
    /// Full-text search over this workspace's entries.
    Search {
        /// What to look for.
        query: String,
        /// Most hits to show.
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Print what an agent would be told right now; for debugging.
    Digest {
        /// Show the delta rather than the full orientation.
        #[arg(long)]
        delta: bool,
        /// Override the character cap.
        #[arg(long)]
        max_chars: Option<usize>,
    },
}

pub fn run(
    command: ContextCommand,
    format: Format,
    workspace: Option<String>,
) -> Result<(), ErrorBody> {
    let mut client = client::connect()?;
    match command {
        ContextCommand::Read { key } => {
            let entry: EntryView =
                client.call("context.read", with(json!({ "key": key }), &workspace))?;
            if format.json {
                output::json(&entry);
            } else {
                println!("{}", entry.value);
            }
        }
        ContextCommand::Write {
            key,
            value,
            tags,
            expected_version,
        } => {
            let written: Written = client.call(
                "context.write",
                with(
                    json!({ "key": key, "value": value, "tags": tags, "expected_version": expected_version }),
                    &workspace,
                ),
            )?;
            if format.json {
                output::json(&written);
            } else {
                println!("v{}", written.version);
            }
        }
        ContextCommand::List { tag } => {
            let list: EntryList =
                client.call("context.list", with(json!({ "tag": tag }), &workspace))?;
            if format.json {
                output::json(&list);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = list
                .entries
                .iter()
                .map(|entry| {
                    vec![
                        entry.key.clone(),
                        entry.author.clone().unwrap_or_else(|| "-".into()),
                        entry.tags.clone().unwrap_or_else(|| "-".into()),
                    ]
                })
                .collect();
            output::table(format, &["KEY", "AUTHOR", "TAGS"], &rows);
        }
        ContextCommand::Note { body, tags } => {
            let appended: Appended = client.call(
                "context.note",
                with(json!({ "body": body.join(" "), "tags": tags }), &workspace),
            )?;
            if format.json {
                output::json(&appended);
            }
        }
        ContextCommand::Send { target, body } => {
            let appended: Appended = client.call(
                "context.message_send",
                with(
                    json!({ "target_agent": target, "body": body.join(" ") }),
                    &workspace,
                ),
            )?;
            if format.json {
                output::json(&appended);
            }
        }
        ContextCommand::Inbox => {
            let inbox: Inbox = client.call("context.inbox", with(json!({}), &workspace))?;
            if format.json {
                output::json(&inbox);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = inbox
                .messages
                .iter()
                .map(|message| {
                    vec![
                        message.from.clone().unwrap_or_else(|| "-".into()),
                        message.body.clone(),
                    ]
                })
                .collect();
            output::table(format, &["FROM", "MESSAGE"], &rows);
        }
        ContextCommand::Search { query, limit } => {
            let results: SearchResults = client.call(
                "context.search",
                with(json!({ "query": query, "limit": limit }), &workspace),
            )?;
            if format.json {
                output::json(&results);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = results
                .hits
                .iter()
                .map(|hit| vec![hit.key.clone(), one_line(&hit.value)])
                .collect();
            output::table(format, &["KEY", "VALUE"], &rows);
        }
        ContextCommand::Digest { delta, max_chars } => {
            let digest: Digest = client.call(
                "context.digest",
                with(
                    json!({
                        "kind": if delta { "delta" } else { "full" },
                        "rate_limited": false,
                        "max_chars": max_chars,
                    }),
                    &workspace,
                ),
            )?;
            if format.json {
                output::json(&digest);
            } else if let Some(text) = digest.text {
                println!("{text}");
            }
        }
    }
    Ok(())
}

/// Adds the caller: the pane this runs in, and the workspace if one was named.
///
/// A `dex context` command run outside a pane needs `--workspace`; the daemon
/// never guesses, because guessing would leak one workspace's context into
/// another (PRD §10.1).
fn with(mut args: Value, workspace: &Option<String>) -> Value {
    let object = args.as_object_mut().expect("args are built as objects");
    if let Ok(pane) = std::env::var("DEX_PANE_ID")
        && !pane.is_empty()
    {
        object.insert("pane".into(), pane.into());
    }
    if let Ok(agent) = std::env::var("DEX_AGENT_ID")
        && !agent.is_empty()
    {
        object.insert("agent".into(), agent.into());
    }
    if let Some(workspace) = workspace {
        object.insert("workspace".into(), workspace.clone().into());
    }
    args
}

/// Values can be paragraphs; a table row shows the first line.
fn one_line(value: &str) -> String {
    let first = value.lines().next().unwrap_or_default();
    if first.chars().count() > 60 {
        format!("{}…", first.chars().take(59).collect::<String>())
    } else {
        first.to_owned()
    }
}
