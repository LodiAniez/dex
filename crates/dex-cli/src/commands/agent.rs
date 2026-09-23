//! `dex agent …`: list and stop the Claude Code sessions running in panes.
//! Spawning arrives in M7.

use std::collections::HashMap;

use clap::{Subcommand, ValueEnum};

/// Where a spawned agent's pane goes.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Direction {
    Right,
    Down,
}
use dex_protocol::ErrorBody;
use dex_protocol::agent::{AgentList, AgentView, Spawned, Stopped};
use dex_protocol::pane::PaneList;
use serde_json::json;

use crate::output::{self, Format};
use dex_cli::client;

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List agents, newest first.
    List {
        /// Only this workspace (name or id).
        #[arg(long)]
        workspace: Option<String>,
        /// Include agents whose session has ended.
        #[arg(long)]
        all: bool,
    },
    /// Stop an agent: presses Ctrl+C in its pane until Claude Code exits, and
    /// closes the pane if Dex made it for a spawned agent.
    Stop {
        /// Agent id, or the label or id of its pane. A label means an agent in
        /// this workspace: the pane's, or the one --workspace names.
        target: String,
        /// Ask it to leave with /exit first, so its SessionEnd hook runs;
        /// Ctrl+C only if it is still there a few seconds later.
        #[arg(long)]
        graceful: bool,
        /// Close its pane whoever made it (a workspace's last pane stays).
        #[arg(long)]
        close_pane: bool,
    },
    /// Start another agent on a task, in its own pane.
    Spawn {
        /// What it should do. It reads this from its workspace context, so it
        /// can be as long as you like.
        #[arg(long)]
        task: String,
        /// Registered repository to work in.
        #[arg(long)]
        repo: Option<String>,
        /// Branch to give it a worktree on; needs --repo.
        #[arg(long)]
        worktree: Option<String>,
        /// Label for its pane.
        #[arg(long)]
        label: Option<String>,
        /// Where its pane goes.
        #[arg(long, value_enum, default_value = "right")]
        direction: Direction,
    },
}

pub fn run(
    command: AgentCommand,
    format: Format,
    workspace: Option<String>,
) -> Result<(), ErrorBody> {
    match command {
        AgentCommand::List { workspace, all } => {
            let mut client = client::connect()?;
            let list: AgentList = client.call(
                "agent.list",
                json!({ "workspace": workspace, "include_dead": all }),
            )?;
            if format.json {
                output::json(&list);
                return Ok(());
            }
            let panes: PaneList = client.call("pane.list", json!({}))?;
            print_agents(&list, &panes, format);
        }
        AgentCommand::Spawn {
            task,
            repo,
            worktree,
            label,
            direction,
        } => {
            let spawned: Spawned = client::connect()?.call(
                "agent.spawn",
                json!({
                    "task": task,
                    "repo": repo,
                    "worktree": worktree,
                    "label": label,
                    "direction": match direction {
                        Direction::Right => "right",
                        Direction::Down => "down",
                    },
                    "pane": std::env::var("DEX_PANE_ID").ok().filter(|id| !id.is_empty()),
                    "workspace": workspace,
                }),
            )?;
            if format.json {
                output::json(&spawned);
            } else {
                println!("{}", spawned.agent);
            }
        }
        AgentCommand::Stop {
            target,
            graceful,
            close_pane,
        } => {
            let stopped: Stopped = client::connect()?.call(
                "agent.stop",
                json!({
                    "agent": target,
                    "graceful": graceful,
                    "close_pane": close_pane,
                    "from_pane": std::env::var("DEX_PANE_ID").ok().filter(|id| !id.is_empty()),
                    "workspace": workspace,
                }),
            )?;
            if format.json {
                output::json(&stopped);
            } else {
                println!("{}", stopped.agent);
                if stopped.closed_pane {
                    eprintln!("closed its pane");
                }
            }
        }
    }
    Ok(())
}

fn print_agents(list: &AgentList, panes: &PaneList, format: Format) {
    let workspaces: HashMap<&str, &str> = panes
        .panes
        .iter()
        .map(|pane| (pane.workspace_id.as_str(), pane.workspace.as_str()))
        .collect();
    let pane_names: HashMap<&str, &str> = panes
        .panes
        .iter()
        .map(|pane| (pane.id.as_str(), pane.label.as_deref().unwrap_or(&pane.id)))
        .collect();
    let rows: Vec<Vec<String>> = list
        .agents
        .iter()
        .map(|agent| {
            let pane = agent
                .pane_id
                .as_deref()
                .and_then(|id| pane_names.get(id).copied());
            vec![
                status_text(agent),
                workspaces
                    .get(agent.workspace_id.as_str())
                    .copied()
                    .unwrap_or("-")
                    .to_owned(),
                pane.unwrap_or("-").to_owned(),
                agent.permission_mode.clone().unwrap_or_else(|| "-".into()),
                agent.id.clone(),
            ]
        })
        .collect();
    output::table(
        format,
        &["STATUS", "WORKSPACE", "PANE", "MODE", "ID"],
        &rows,
    );
}

/// `running`, or `error (rate_limit)` when there is a reason.
fn status_text(agent: &AgentView) -> String {
    let name = serde_json::to_value(agent.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    // A working agent that has sent no hook for a while is inside one long
    // tool call, which is worth knowing and is nobody's fault (issue #67).
    let quiet = match agent.quiet_for_ms {
        Some(ms) => format!(", quiet {}", quiet_span(ms)),
        None => String::new(),
    };
    match &agent.status_detail {
        // What an idle agent said last is for the office; what it asked is worth a column.
        Some(detail) if detail.starts_with("said: ") => format!("{name}{quiet}"),
        Some(detail) => format!("{name} ({detail}){quiet}"),
        None => format!("{name}{quiet}"),
    }
}

/// How long a silence has lasted: `40s`, `12m`, `1h 3m`.
fn quiet_span(ms: i64) -> String {
    let seconds = ms.max(0) / 1_000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    format!("{}h {}m", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::quiet_span;

    #[test]
    fn a_silence_is_counted_in_seconds_then_minutes_then_hours() {
        assert_eq!(quiet_span(40_000), "40s");
        assert_eq!(quiet_span(12 * 60_000), "12m");
        assert_eq!(quiet_span(63 * 60_000), "1h 3m");
    }

    #[test]
    fn a_silence_never_reads_as_negative_whatever_the_clocks_say() {
        assert_eq!(quiet_span(-5_000), "0s");
    }
}
