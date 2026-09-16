//! `dex pane …`: list, create, split, close, send, send-key, label, id.
//!
//! "The current pane" is the one this command runs in, from `DEX_PANE_ID`,
//! which Dex sets in every pane's shell. `--target` takes a label or an id.

use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};
use dex_protocol::pane::{Created, PaneList, Sent};
use dex_protocol::workspace::WorkspaceList;
use dex_protocol::{ErrorBody, ErrorCode};
use serde_json::json;

use crate::commands::workspace::absolute;
use crate::output::{self, Format};
use dex_cli::client;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Direction {
    Right,
    Down,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum KeyName {
    Enter,
    Tab,
    Escape,
    CtrlC,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Subcommand)]
pub enum PaneCommand {
    /// List panes (* marks each workspace's focused pane).
    List {
        /// Only this workspace (name or id).
        #[arg(long, conflicts_with = "current")]
        workspace: Option<String>,
        /// Only the workspace this command runs in.
        #[arg(long)]
        current: bool,
    },
    /// Open a new pane, split from a workspace's focused pane.
    Create {
        /// Working directory (default: the split pane's).
        #[arg(long)]
        path: Option<PathBuf>,
        /// Label, so other commands can target the pane by name.
        #[arg(long)]
        label: Option<String>,
        /// Workspace name or id (default: the current pane's, else the one on screen).
        #[arg(long)]
        workspace: Option<String>,
        /// `terminal` (default), `activity` (the workspace's event stream),
        /// `diff` (the changes in --path's repository), or `markdown` (the
        /// file at --path, rendered).
        #[arg(long)]
        kind: Option<String>,
    },
    /// Split the current pane.
    Split {
        #[arg(long, value_enum, default_value = "right")]
        direction: Direction,
        /// Working directory (default: the current pane's).
        #[arg(long)]
        path: Option<PathBuf>,
        /// Label for the new pane.
        #[arg(long)]
        label: Option<String>,
        /// `terminal` (default), `activity` (the workspace's event stream),
        /// `diff` (the changes in --path's repository), or `markdown` (the
        /// file at --path, rendered).
        #[arg(long)]
        kind: Option<String>,
    },
    /// Close a pane.
    Close {
        /// Pane label or id (default: the current pane).
        #[arg(long)]
        target: Option<String>,
    },
    /// Type text into a pane's shell, then press Enter.
    Send {
        /// Pane label or id (default: the current pane).
        #[arg(long)]
        target: Option<String>,
        /// Don't press Enter afterwards.
        #[arg(long)]
        bare: bool,
        /// The text; words are joined with spaces.
        #[arg(required = true, num_args = 1.., allow_hyphen_values = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Press one key in a pane.
    SendKey {
        /// Pane label or id (default: the current pane).
        #[arg(long)]
        target: Option<String>,
        #[arg(value_enum)]
        key: KeyName,
    },
    /// Label the current pane, so other commands can target it by name.
    Label {
        /// 1–32 characters, no spaces.
        name: String,
    },
    /// Print the current pane's id.
    Id,
}

pub fn run(command: PaneCommand, format: Format) -> Result<(), ErrorBody> {
    match command {
        PaneCommand::Id => print_pane(&current_pane()?, format),
        PaneCommand::List { workspace, current } => {
            let pane = if current { Some(current_pane()?) } else { None };
            let list: PaneList = client::connect()?
                .call("pane.list", json!({ "workspace": workspace, "pane": pane }))?;
            print_panes(&list, format);
        }
        PaneCommand::Create {
            path,
            label,
            workspace,
            kind,
        } => {
            let context = std::env::var("DEX_PANE_ID")
                .ok()
                .filter(|id| !id.is_empty());
            let created: Created = client::connect()?.call(
                "pane.create",
                json!({ "workspace": workspace, "pane": context, "cwd": path.map(absolute), "label": label, "kind": kind }),
            )?;
            print_pane(&created.pane, format);
        }
        PaneCommand::Split {
            direction,
            path,
            label,
            kind,
        } => {
            let pane = current_pane()?;
            let direction = match direction {
                Direction::Right => "right",
                Direction::Down => "down",
            };
            let list: WorkspaceList = client::connect()?.call(
                "pane.split",
                json!({ "pane": pane, "direction": direction, "cwd": path.map(absolute), "label": label, "kind": kind }),
            )?;
            // The new pane takes focus in the split pane's workspace.
            let new = list
                .workspaces
                .iter()
                .find(|ws| ws.panes.iter().any(|p| p.id == pane))
                .and_then(|ws| ws.active_pane.clone())
                .unwrap_or_default();
            print_pane(&new, format);
        }
        PaneCommand::Close { target } => {
            let _: WorkspaceList = client::connect()?
                .call("pane.close", json!({ "pane": target_or_current(target)? }))?;
        }
        PaneCommand::Send { target, bare, text } => {
            let sent: Sent = client::connect()?.call(
                "pane.send",
                json!({ "pane": target_or_current(target)?, "text": text.join(" "), "enter": !bare }),
            )?;
            if format.json {
                output::json(&sent);
            }
        }
        PaneCommand::SendKey { target, key } => {
            let sent: Sent = client::connect()?.call(
                "pane.send_key",
                json!({ "pane": target_or_current(target)?, "key": key_name(key) }),
            )?;
            if format.json {
                output::json(&sent);
            }
        }
        PaneCommand::Label { name } => {
            let _: WorkspaceList = client::connect()?.call(
                "pane.label",
                json!({ "pane": current_pane()?, "label": name }),
            )?;
        }
    }
    Ok(())
}

/// The pane this command runs in.
fn current_pane() -> Result<String, ErrorBody> {
    std::env::var("DEX_PANE_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ErrorBody {
            code: ErrorCode::NotInPane,
            message: "this is not running inside a Dex pane".into(),
            repair:
                "Run it inside a Dex pane, or pass --target <label-or-id> (see `dex pane list`)."
                    .into(),
        })
}

fn target_or_current(target: Option<String>) -> Result<String, ErrorBody> {
    match target {
        Some(target) => Ok(target),
        None => current_pane(),
    }
}

fn key_name(key: KeyName) -> &'static str {
    match key {
        KeyName::Enter => "enter",
        KeyName::Tab => "tab",
        KeyName::Escape => "escape",
        KeyName::CtrlC => "ctrl-c",
        KeyName::Up => "up",
        KeyName::Down => "down",
        KeyName::Left => "left",
        KeyName::Right => "right",
    }
}

fn print_pane(id: &str, format: Format) {
    if format.json {
        output::json(&json!({ "pane": id }));
    } else {
        println!("{id}");
    }
}

fn print_panes(list: &PaneList, format: Format) {
    if format.json {
        return output::json(list);
    }
    let rows: Vec<Vec<String>> = list
        .panes
        .iter()
        .map(|pane| {
            vec![
                if pane.focused {
                    "*".into()
                } else {
                    String::new()
                },
                pane.workspace.clone(),
                pane.label.clone().unwrap_or_else(|| "-".into()),
                pane.cwd.clone(),
                pane.id.clone(),
            ]
        })
        .collect();
    output::table(format, &["", "WORKSPACE", "LABEL", "CWD", "ID"], &rows);
}
