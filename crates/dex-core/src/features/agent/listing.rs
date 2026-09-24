//! `agent.list`: who is in the workspace, and what each client is told about
//! them. Split from `commands.rs` to keep both files inside the 400-line rule.

use dex_protocol::agent::{AgentList, AgentStatus, AgentView, ListAgentsArgs};

use super::model::{Agent, AgentError};
use super::{silence, store};
use crate::app::AppState;
use crate::features::{context, workspace};
use crate::platform::clock;

/// `agent.list`: agents newest first, optionally including ended ones and
/// limited to one workspace (by id or name).
pub async fn list(state: &AppState, args: ListAgentsArgs) -> Result<AgentList, AgentError> {
    // The owner's taste for how long a working agent may go quiet before Dex
    // says so; read once, so every agent in one listing is judged alike.
    let quiet_after_ms =
        (state.config.get().agents.quiet_after_seconds as i64).saturating_mul(1_000);
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<AgentList, AgentError>> {
                let scope = match (args.workspace.as_deref(), args.pane.as_deref()) {
                    (Some(target), _) => match workspace::workspace_id(conn, target)? {
                        Ok(id) => Some(id),
                        Err(err) => return Ok(Err(err.into())),
                    },
                    // A caller in a pane sees its own workspace's agents only.
                    (None, Some(pane)) => match workspace::find_pane_workspace(conn, pane)? {
                        Some(id) => Some(id),
                        None => return Ok(Err(AgentError::NoSuchPane(pane.to_owned()))),
                    },
                    (None, None) => None,
                };
                let started = store::started_ids(conn)?;
                let now = clock::now_millis();
                let unread = context::unread_counts(conn)?;
                // An agent asking - one runs in the pane the request came from -
                // is told that a colleague is waiting, not what for: the reason
                // can name a file or a command, and is the owner's to see.
                let asker = match args.pane.as_deref() {
                    Some(pane) => store::find_live_in_pane(conn, pane)?.map(|agent| agent.id),
                    None => None,
                };
                let agents = store::list_agents(conn, args.include_dead)?
                    .into_iter()
                    .filter(|agent| scope.as_ref().is_none_or(|id| id == &agent.workspace_id))
                    .map(|agent| {
                        let has_started = started.contains(&agent.id);
                        let waiting = unread.get(&agent.id).copied().unwrap_or(0);
                        let quiet = silence::quiet_for(
                            agent.status,
                            agent.last_event_at,
                            now,
                            quiet_after_ms,
                        );
                        let mut seen = view(agent, has_started, waiting, quiet);
                        let theirs = asker.as_ref().is_none_or(|id| id == &seen.id);
                        // Nor what a colleague asked the owner: it may quote anything.
                        if !theirs
                            && matches!(seen.status, AgentStatus::Waiting | AgentStatus::Idle)
                        {
                            seen.status_detail = None;
                        }
                        seen
                    })
                    .collect();
                Ok(Ok(AgentList {
                    agents,
                    revision: store::revision(conn)?,
                }))
            },
        )
        .await?
}

fn view(agent: Agent, started: bool, unread: usize, quiet_for_ms: Option<i64>) -> AgentView {
    AgentView {
        last_event_at: agent.last_event_at,
        id: agent.id,
        pane_id: agent.pane_id,
        workspace_id: agent.workspace_id,
        label: agent.label,
        unread,
        backend: agent.backend,
        status: agent.status,
        status_detail: agent.status_detail,
        status_at: agent.status_at,
        quiet_for_ms,
        permission_mode: agent.permission_mode,
        task_brief: agent.task_brief,
        started,
        parent_id: agent.parent_id,
        depth: agent.depth,
        started_at: agent.started_at,
        ended_at: agent.ended_at,
    }
}
