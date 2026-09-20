//! Every SQL statement touching `agent`. No other module queries it
//! (docs/conventions.md §1.3).

use dex_protocol::agent::AgentStatus;
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::logic::{status_from_name, status_name};
use super::model::Agent;

const AGENT_COLUMNS: &str = "id, pane_id, workspace_id, label, backend, status, status_detail, \
     status_at, permission_mode, task_brief, started_at, last_event_at, ended_at, parent_id, depth";

fn agent_from_row(row: &Row<'_>) -> rusqlite::Result<Agent> {
    Ok(Agent {
        id: row.get(0)?,
        pane_id: row.get(1)?,
        workspace_id: row.get(2)?,
        label: row.get(3)?,
        backend: row.get(4)?,
        status: status_from_name(&row.get::<_, String>(5)?),
        status_detail: row.get(6)?,
        status_at: row.get(7)?,
        permission_mode: row.get(8)?,
        task_brief: row.get(9)?,
        started_at: row.get(10)?,
        last_event_at: row.get(11)?,
        ended_at: row.get(12)?,
        parent_id: row.get(13)?,
        depth: row.get(14)?,
    })
}

fn find_one(
    conn: &Connection,
    filter: &str,
    values: impl rusqlite::Params,
) -> rusqlite::Result<Option<Agent>> {
    conn.query_row(
        &format!("SELECT {AGENT_COLUMNS} FROM agent WHERE {filter} ORDER BY started_at DESC, rowid DESC LIMIT 1"),
        values,
        agent_from_row,
    )
    .optional()
}

/// Inserts an agent. A spawned one carries its parent and depth; one a human
/// started has neither.
pub fn insert_agent(
    conn: &Connection,
    agent: &Agent,
    session_id: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO agent (id, pane_id, workspace_id, parent_id, label, backend, session_id,
                            status, status_detail, status_at, permission_mode, task_brief, depth,
                            started_at, last_event_at, ended_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            agent.id,
            agent.pane_id,
            agent.workspace_id,
            agent.parent_id,
            agent.label,
            agent.backend,
            session_id,
            status_name(agent.status),
            agent.status_detail,
            agent.status_at,
            agent.permission_mode,
            agent.task_brief,
            agent.depth,
            agent.started_at,
            agent.last_event_at,
            agent.ended_at,
        ],
    )?;
    Ok(())
}

/// A live agent in a pane that has not yet bound to a backend session: the row
/// `agent.spawn` created before its Claude Code started.
pub fn find_unbound_in_pane(conn: &Connection, pane_id: &str) -> rusqlite::Result<Option<Agent>> {
    find_one(
        conn,
        "pane_id = ?1 AND session_id IS NULL AND status != 'dead'",
        [pane_id],
    )
}

/// How many agents in a workspace have not ended.
pub fn count_live_in_workspace(conn: &Connection, workspace_id: &str) -> rusqlite::Result<i64> {
    // An agent whose pane is gone cannot be running anything, whatever its
    // status says; `end_orphans` catches up with it, but the limit must not
    // wait for that (ARCHITECTURE.md: stale agents counted toward the limit).
    conn.query_row(
        "SELECT COUNT(*) FROM agent
         WHERE workspace_id = ?1 AND status != 'dead' AND pane_id IS NOT NULL",
        [workspace_id],
        |row| row.get(0),
    )
}

/// Ends every agent whose pane no longer exists. `pane_id` is set null by the
/// schema when a pane is deleted (closed, or gone with its workspace), which
/// says nothing about status; this is where status catches up. Returns how
/// many were ended.
pub fn end_orphans(conn: &Connection, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE agent SET status = 'dead', status_at = ?1, ended_at = COALESCE(ended_at, ?1),
                          status_detail = COALESCE(status_detail, 'pane closed')
         WHERE pane_id IS NULL AND status != 'dead'",
        params![now],
    )
}

/// One agent by id.
pub fn find_agent(conn: &Connection, id: &str) -> rusqlite::Result<Option<Agent>> {
    find_one(conn, "id = ?1", [id])
}

/// The agent for a Claude Code session in a pane.
pub fn find_by_session(
    conn: &Connection,
    pane_id: &str,
    session_id: &str,
) -> rusqlite::Result<Option<Agent>> {
    find_one(
        conn,
        "pane_id = ?1 AND session_id = ?2",
        [pane_id, session_id],
    )
}

/// The agent in a pane that ended most recently.
pub fn find_last_ended_in_pane(
    conn: &Connection,
    pane_id: &str,
) -> rusqlite::Result<Option<Agent>> {
    conn.query_row(
        &format!(
            "SELECT {AGENT_COLUMNS} FROM agent WHERE pane_id = ?1 AND status = 'dead'
             ORDER BY status_at DESC, rowid DESC LIMIT 1"
        ),
        [pane_id],
        agent_from_row,
    )
    .optional()
}

/// The newest agent in a pane that has not ended.
pub fn find_live_in_pane(conn: &Connection, pane_id: &str) -> rusqlite::Result<Option<Agent>> {
    find_one(conn, "pane_id = ?1 AND status != 'dead'", [pane_id])
}

/// The newest live agent with this label.
pub fn list_live_by_label(
    conn: &Connection,
    label: &str,
    within: Option<&str>,
) -> rusqlite::Result<Vec<Agent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {AGENT_COLUMNS} FROM agent
         WHERE label = ?1 AND status != 'dead' AND (?2 IS NULL OR workspace_id = ?2)
         ORDER BY started_at, rowid"
    ))?;
    let rows = stmt.query_map(params![label, within], agent_from_row)?;
    rows.collect()
}

/// Binds an agent to a (possibly new) session id and records the permission mode.
pub fn update_session(
    conn: &Connection,
    id: &str,
    session_id: Option<&str>,
    permission_mode: Option<&str>,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE agent SET session_id = COALESCE(?2, session_id),
                          permission_mode = COALESCE(?3, permission_mode),
                          last_event_at = ?4
         WHERE id = ?1",
        params![id, session_id, permission_mode, now],
    )?;
    Ok(())
}

/// Records that a hook arrived, and the permission mode it reported.
pub fn touch(
    conn: &Connection,
    id: &str,
    permission_mode: Option<&str>,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE agent SET last_event_at = ?2, permission_mode = COALESCE(?3, permission_mode)
         WHERE id = ?1",
        params![id, now, permission_mode],
    )?;
    Ok(())
}

/// Moves an agent to a new status. `ended_at` is kept if already set.
pub fn update_status(
    conn: &Connection,
    id: &str,
    status: AgentStatus,
    detail: Option<&str>,
    status_at: i64,
    ended_at: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE agent SET status = ?2, status_detail = ?3, status_at = ?4,
                          ended_at = COALESCE(ended_at, ?5)
         WHERE id = ?1",
        params![id, status_name(status), detail, status_at, ended_at],
    )?;
    Ok(())
}

/// Undoes an end: clears `ended_at` and parks the agent in `unknown` until the
/// hook that proved it alive sets its real status.
pub fn revive(conn: &Connection, id: &str, status_at: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE agent SET status = 'unknown', status_at = ?2, ended_at = NULL WHERE id = ?1",
        params![id, status_at],
    )?;
    Ok(())
}

/// Ends one agent. Returns how many rows changed.
pub fn end_agent(conn: &Connection, id: &str, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE agent SET status = 'dead', status_at = ?2, ended_at = COALESCE(ended_at, ?2)
         WHERE id = ?1",
        params![id, now],
    )
}

/// Ends every live agent in a pane, except `keep`. Returns how many ended.
pub fn end_live_in_pane(
    conn: &Connection,
    pane_id: &str,
    keep: Option<&str>,
    now: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE agent SET status = 'dead', status_at = ?3, ended_at = COALESCE(ended_at, ?3)
         WHERE pane_id = ?1 AND status != 'dead' AND id IS NOT ?2",
        params![pane_id, keep, now],
    )
}

/// Agents newest first; ended ones only if asked.
pub fn list_agents(conn: &Connection, include_dead: bool) -> rusqlite::Result<Vec<Agent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {AGENT_COLUMNS} FROM agent WHERE ?1 OR status != 'dead'
         ORDER BY started_at DESC, rowid DESC"
    ))?;
    let rows = stmt.query_map([include_dead], agent_from_row)?;
    rows.collect()
}

/// Every agent in a given status.
pub fn list_by_status(conn: &Connection, status: AgentStatus) -> rusqlite::Result<Vec<Agent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {AGENT_COLUMNS} FROM agent WHERE status = ?1"
    ))?;
    let rows = stmt.query_map([status_name(status)], agent_from_row)?;
    rows.collect()
}

/// A number that grows with every row this connection changes; orders snapshots.
pub fn revision(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT total_changes()", [], |row| row.get(0))
}

/// Whether Claude Code has started in this agent's pane: `SessionStart` set a
/// session id. A spawned row has none until then.
/// Whether `agent_id` was spawned by `ancestor_id`, directly or through its
/// own spawns. Walks up the parents; the depth limit keeps that short, and the
/// bound keeps a corrupt cycle from walking for ever.
pub fn descends_from(
    conn: &Connection,
    agent_id: &str,
    ancestor_id: &str,
) -> rusqlite::Result<bool> {
    let mut current = agent_id.to_owned();
    for _ in 0..32 {
        let parent: Option<Option<String>> = conn
            .query_row(
                "SELECT parent_id FROM agent WHERE id = ?1",
                [&current],
                |row| row.get(0),
            )
            .optional()?;
        match parent.flatten() {
            Some(parent) if parent == ancestor_id => return Ok(true),
            Some(parent) => current = parent,
            None => return Ok(false),
        }
    }
    Ok(false)
}

/// The ids of every agent whose Claude Code has started, in one query: what
/// `has_session` answers for one agent, for a whole list.
pub fn started_ids(conn: &Connection) -> rusqlite::Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT id FROM agent WHERE session_id IS NOT NULL")?;
    let mut ids = std::collections::HashSet::new();
    for id in stmt.query_map([], |row| row.get(0))? {
        ids.insert(id?);
    }
    Ok(ids)
}

/// Living agents whose Claude Code has started, with the pane each runs in:
/// those the process table can be asked about.
pub fn presence_candidates(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, pane_id FROM agent
         WHERE status != 'dead' AND session_id IS NOT NULL AND pane_id IS NOT NULL
         ORDER BY started_at, rowid",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

/// The panes of the living agents in `workspace_id` that share `parent` - the
/// caller's earlier hires - in the order they were hired, leaving out
/// `caller_pane` itself. `None` is the owner: hires from HR or the CLI, which
/// Dex spawned (depth above 0) - not agents the owner started by typing
/// `claude`, which are nobody's hires and must not be split into.
pub fn live_sibling_panes(
    conn: &Connection,
    workspace_id: &str,
    parent: Option<&str>,
    caller_pane: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT pane_id FROM agent
         WHERE workspace_id = ?1 AND status != 'dead' AND pane_id IS NOT NULL AND pane_id != ?3
           AND ((?2 IS NULL AND parent_id IS NULL AND depth > 0) OR parent_id = ?2)
         ORDER BY started_at, rowid",
    )?;
    let rows = stmt.query_map(params![workspace_id, parent, caller_pane], |row| row.get(0))?;
    rows.collect()
}

pub fn has_session(conn: &Connection, agent_id: &str) -> rusqlite::Result<bool> {
    let started: Option<bool> = conn
        .query_row(
            "SELECT session_id IS NOT NULL FROM agent WHERE id = ?1",
            [agent_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(started.unwrap_or(false))
}
