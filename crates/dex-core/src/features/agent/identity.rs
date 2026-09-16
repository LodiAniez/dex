//! Naming and finding agents: the questions other slices ask about them,
//! and the target resolution `agent.stop` shares with them.

use rusqlite::Connection;

use super::logic;
use super::model::{Agent, AgentError};
use super::store;
use crate::features::workspace::{self, WorkspaceError};

/// For other slices: a display label for an agent — its own label, else its
/// pane's label, else a short form of its id, so activity is readable.
pub fn label_of(conn: &Connection, agent_id: &str) -> rusqlite::Result<Option<String>> {
    let Some(agent) = store::find_agent(conn, agent_id)? else {
        return Ok(None);
    };
    if let Some(label) = agent.label {
        return Ok(Some(label));
    }
    let from_pane = match agent.pane_id.as_deref() {
        Some(pane) => workspace::pane_label(conn, pane)?,
        None => None,
    };
    Ok(Some(from_pane.unwrap_or_else(|| short_id(&agent.id))))
}

/// For other slices: what an agent was spawned to do.
pub fn brief_of(conn: &Connection, agent_id: &str) -> rusqlite::Result<Option<String>> {
    Ok(store::find_agent(conn, agent_id)?.and_then(|agent| agent.task_brief))
}

/// For other slices: the live agents of a workspace other than `exclude`, as
/// `(label, status, task_brief)` — who else is working, and on what.
pub fn siblings(
    conn: &Connection,
    workspace_id: &str,
    exclude: Option<&str>,
) -> rusqlite::Result<Vec<(String, String, Option<String>)>> {
    let mut found = Vec::new();
    for agent in store::list_agents(conn, false)? {
        if agent.workspace_id != workspace_id || exclude == Some(agent.id.as_str()) {
            continue;
        }
        let label = label_of(conn, &agent.id)?.unwrap_or_else(|| short_id(&agent.id));
        found.push((
            label,
            logic::status_name(agent.status).to_owned(),
            agent.task_brief,
        ));
    }
    Ok(found)
}

/// For other slices: where an agent is and what it is doing — its pane, if it
/// still has one, and its status. `None` for an id nobody has.
pub fn whereabouts(
    conn: &Connection,
    agent_id: &str,
) -> rusqlite::Result<Option<(Option<String>, dex_protocol::agent::AgentStatus)>> {
    Ok(store::find_agent(conn, agent_id)?.map(|agent| (agent.pane_id, agent.status)))
}

/// For other slices: the id of the agent a target names, or `None`.
pub fn resolve_agent(conn: &Connection, target: &str) -> rusqlite::Result<Option<String>> {
    Ok(find_target(conn, target)?.ok().map(|agent| agent.id))
}

/// The first block of a uuid, which is enough to tell agents apart by eye.
fn short_id(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_owned()
}

/// The agent a target names: its id, its label, or a pane (id or label) whose
/// live agent it is.
pub(super) fn find_target(
    conn: &Connection,
    target: &str,
) -> rusqlite::Result<Result<Agent, AgentError>> {
    if let Some(agent) = store::find_agent(conn, target)? {
        return Ok(Ok(agent));
    }
    if let Some(agent) = store::find_live_by_label(conn, target)? {
        return Ok(Ok(agent));
    }
    let missing = || AgentError::NoSuchAgent(target.to_owned());
    Ok(match workspace::pane_id(conn, target)? {
        Ok(pane) => store::find_live_in_pane(conn, &pane)?.ok_or_else(missing),
        Err(WorkspaceError::NoSuchPane(_)) => Err(missing()),
        Err(err) => Err(err.into()),
    })
}
