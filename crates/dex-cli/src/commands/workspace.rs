//! `dex workspace …`: list, create, switch, delete.

use std::path::PathBuf;

use clap::Subcommand;
use dex_protocol::ErrorBody;
use dex_protocol::workspace::WorkspaceList;
use serde_json::json;

use crate::client;
use crate::output::{self, Format};

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// List workspaces in sidebar order (* marks the one on screen).
    List,
    /// Create a workspace; it becomes the one on screen.
    Create {
        /// Display name (default: "Workspace N").
        #[arg(long)]
        name: Option<String>,
        /// Root folder (default: your home folder).
        #[arg(long)]
        path: Option<PathBuf>,
        /// Color as #rrggbb (default: the next palette color).
        #[arg(long)]
        color: Option<String>,
    },
    /// Show a workspace in the app.
    Switch {
        /// Workspace name or id.
        target: String,
    },
    /// Delete a workspace and close its terminals.
    Delete {
        /// Workspace name or id.
        target: String,
    },
}

pub fn run(command: WorkspaceCommand, format: Format) -> Result<(), ErrorBody> {
    let mut client = client::connect()?;
    let list: WorkspaceList = match command {
        WorkspaceCommand::List => client.call("workspace.list", json!({}))?,
        WorkspaceCommand::Create { name, path, color } => client.call(
            "workspace.create",
            json!({ "name": name, "root_path": path.map(absolute), "color": color }),
        )?,
        WorkspaceCommand::Switch { target } => {
            client.call("workspace.switch", json!({ "workspace": target }))?
        }
        WorkspaceCommand::Delete { target } => {
            client.call("workspace.delete", json!({ "workspace": target }))?
        }
    };
    print_list(&list, format);
    Ok(())
}

/// `path` made absolute against this shell's directory: the app runs elsewhere.
pub fn absolute(path: PathBuf) -> String {
    let full = if path.is_absolute() {
        path
    } else {
        match std::env::current_dir() {
            Ok(dir) => dir.join(path),
            Err(_) => path,
        }
    };
    full.to_string_lossy().into_owned()
}

fn print_list(list: &WorkspaceList, format: Format) {
    if format.json {
        return output::json(list);
    }
    let rows: Vec<Vec<String>> = list
        .workspaces
        .iter()
        .map(|ws| {
            let active = if list.active.as_deref() == Some(ws.id.as_str()) {
                "*"
            } else {
                ""
            };
            vec![
                active.to_owned(),
                ws.name.clone(),
                ws.panes.len().to_string(),
                ws.color.clone().unwrap_or_else(|| "-".into()),
                ws.root_path.clone(),
                ws.id.clone(),
            ]
        })
        .collect();
    output::table(format, &["", "NAME", "PANES", "COLOR", "ROOT", "ID"], &rows);
}
