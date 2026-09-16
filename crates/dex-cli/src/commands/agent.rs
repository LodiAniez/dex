//! `dex agent …`: list and stop the Claude Code sessions running in panes.
//! Spawning arrives in M7.

use std::collections::HashMap;

use clap::Subcommand;
use dex_protocol::ErrorBody;
use dex_protocol::agent::{AgentList, AgentView, Stopped};
use dex_protocol::pane::PaneList;
use serde_json::json;

use crate::client;
use crate::output::{self, Format};

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
    /// Stop an agent: presses Ctrl+C in its pane until Claude Code exits.
    Stop {
        /// Agent id, or the label or id of its pane.
        target: String,
    },
}

pub fn run(command: AgentCommand, format: Format) -> Result<(), ErrorBody> {
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
        AgentCommand::Stop { target } => {
            let stopped: Stopped =
                client::connect()?.call("agent.stop", json!({ "agent": target }))?;
            if format.json {
                output::json(&stopped);
            } else {
                println!("{}", stopped.agent);
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
    match &agent.status_detail {
        Some(detail) => format!("{name} ({detail})"),
        None => name,
    }
}
