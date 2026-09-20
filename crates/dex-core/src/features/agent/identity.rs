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

/// For other slices: where an agent is and what it is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Whereabouts {
    /// Its pane, if it still has one.
    pub pane_id: Option<String>,
    pub status: dex_protocol::agent::AgentStatus,
    /// Whether Claude Code has actually started in that pane (`SessionStart`
    /// has been seen). A spawned row is `idle` from birth; until it has a
    /// session that means "not started yet", not "sitting at its prompt".
    pub started: bool,
    /// Whether its turn ended on a question for the owner: the pane is waiting
    /// for their answer, and anything typed would be taken as it.
    pub asked_owner: bool,
    /// When it last changed status.
    pub idle_at: i64,
}

/// For other slices: where an agent is and what it is doing. `None` for an id
/// nobody has.
pub fn whereabouts(conn: &Connection, agent_id: &str) -> rusqlite::Result<Option<Whereabouts>> {
    let Some(agent) = store::find_agent(conn, agent_id)? else {
        return Ok(None);
    };
    Ok(Some(Whereabouts {
        pane_id: agent.pane_id,
        status: agent.status,
        started: store::has_session(conn, &agent.id)?,
        asked_owner: super::asking::asked_the_owner(agent.status_detail.as_deref()),
        idle_at: agent.status_at,
    }))
}

/// For other slices: the ids of a workspace's agents that have ended.
pub fn ended_in_workspace(conn: &Connection, workspace_id: &str) -> rusqlite::Result<Vec<String>> {
    Ok(store::list_agents(conn, true)?
        .into_iter()
        .filter(|agent| {
            agent.workspace_id == workspace_id
                && agent.status == dex_protocol::agent::AgentStatus::Dead
        })
        .map(|agent| agent.id)
        .collect())
}

/// For other slices: the id of the agent a target names, or `None`. `within`
/// is the caller's workspace, which is where a label is looked for.
pub fn resolve_agent(
    conn: &Connection,
    target: &str,
    within: Option<&str>,
) -> rusqlite::Result<Option<String>> {
    Ok(find_target(conn, target, within)?
        .ok()
        .map(|agent| agent.id))
}

/// For this slice's commands: the workspace a request speaks from - the one
/// it named, else the one holding the pane it came from. `None` when it has
/// neither, which is a `dex` command run outside any pane.
///
/// A named workspace wins over the caller's pane, as it does for `context`:
/// naming one is asking for it. `agent.spawn` reads them the other way round
/// (`spawn::caller`), because a spawned agent's pane is split from the
/// caller's, and it must be split where the caller is.
pub fn caller_workspace(
    conn: &Connection,
    workspace: Option<&str>,
    from_pane: Option<&str>,
) -> rusqlite::Result<Result<Option<String>, AgentError>> {
    if let Some(target) = workspace {
        return Ok(match workspace::workspace_id(conn, target)? {
            Ok(id) => Ok(Some(id)),
            Err(err) => Err(err.into()),
        });
    }
    let Some(pane) = from_pane else {
        return Ok(Ok(None));
    };
    Ok(Ok(workspace::find_pane_workspace(conn, pane)?))
}

/// The first block of a uuid, which is enough to tell agents apart by eye.
fn short_id(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_owned()
}

/// The agent a target names: its id, its label, or a pane (id or label) whose
/// live agent it is.
///
/// `within` is the caller's workspace, and labels are looked for inside it:
/// they are unique per workspace, not across them, so two workspaces may each
/// have a `reviewer` at work (issue #59). An id is unique everywhere and names
/// its agent from anywhere. With no workspace to go on - a `dex` command run
/// outside any pane - a label that matches in several workspaces is refused
/// rather than guessed at, and the caller is asked to say which.
pub(super) fn find_target(
    conn: &Connection,
    target: &str,
    within: Option<&str>,
) -> rusqlite::Result<Result<Agent, AgentError>> {
    if let Some(agent) = store::find_agent(conn, target)? {
        return Ok(Ok(agent));
    }
    let mut labelled = store::list_live_by_label(conn, target, within)?;
    // Several matches in one workspace is old data, not an ambiguous request:
    // an agent keeps the label it was hired with, so relabelling its pane and
    // hiring again leaves two. The newest is what Dex has always taken, and
    // asking for a workspace would not narrow it.
    if within.is_none() && labelled.len() > 1 {
        return Ok(Err(in_several_workspaces(conn, target, &labelled)?));
    }
    if let Some(agent) = labelled.pop() {
        return Ok(Ok(agent));
    }
    let missing = || AgentError::NoSuchAgent(target.to_owned());
    Ok(match workspace::pane_id_in(conn, target, within)? {
        Ok(pane) => store::find_live_in_pane(conn, &pane)?.ok_or_else(missing),
        Err(WorkspaceError::NoSuchPane(_)) => Err(missing()),
        Err(err) => Err(err.into()),
    })
}

/// Which agents a label matched, and where, so the caller can name one
/// workspace and mean one agent.
fn in_several_workspaces(
    conn: &Connection,
    target: &str,
    matched: &[Agent],
) -> rusqlite::Result<AgentError> {
    let mut candidates = Vec::new();
    for agent in matched {
        let workspace = workspace::workspace_name(conn, &agent.workspace_id)?
            .unwrap_or_else(|| agent.workspace_id.clone());
        // The whole id: a candidate is meant to be pasted back as a target.
        candidates.push(format!("{} in {workspace}", agent.id));
    }
    Ok(AgentError::AmbiguousTarget {
        target: target.to_owned(),
        candidates,
    })
}
