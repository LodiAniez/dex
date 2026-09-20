//! Every SQL statement touching `context_entry`, `context_event` and
//! `context_cursor`. No other module queries them (docs/conventions.md §1.3).

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::model::{Entry, Event, NewEvent};

const ENTRY_COLUMNS: &str = "id, workspace_id, key, value, version, author_agent, tags, updated_at";
const EVENT_COLUMNS: &str =
    "seq, workspace_id, agent_id, kind, key, body, target_agent, read_at, created_at";

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        key: row.get(2)?,
        value: row.get(3)?,
        version: row.get(4)?,
        author_agent: row.get(5)?,
        tags: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn event_from_row(row: &Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        seq: row.get(0)?,
        workspace_id: row.get(1)?,
        agent_id: row.get(2)?,
        kind: row.get(3)?,
        key: row.get(4)?,
        body: row.get(5)?,
        target_agent: row.get(6)?,
        read_at: row.get(7)?,
        created_at: row.get(8)?,
    })
}

/// One entry by key, within a workspace.
pub fn find_entry(
    conn: &Connection,
    workspace_id: &str,
    key: &str,
) -> rusqlite::Result<Option<Entry>> {
    conn.query_row(
        &format!("SELECT {ENTRY_COLUMNS} FROM context_entry WHERE workspace_id = ?1 AND key = ?2"),
        [workspace_id, key],
        entry_from_row,
    )
    .optional()
}

/// Inserts a new entry at version 1 and returns that version.
pub fn insert_entry(
    conn: &Connection,
    workspace_id: &str,
    key: &str,
    value: &str,
    author: Option<&str>,
    tags: Option<&str>,
    now: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO context_entry (workspace_id, key, value, version, author_agent, tags, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6)",
        params![workspace_id, key, value, author, tags, now],
    )?;
    Ok(1)
}

/// Replaces an entry's value and bumps its version, returning the new version.
/// Tags are replaced only when given, so a write can leave them alone.
pub fn update_entry(
    conn: &Connection,
    id: i64,
    value: &str,
    author: Option<&str>,
    tags: Option<&str>,
    now: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "UPDATE context_entry
         SET value = ?2, version = version + 1, author_agent = ?3,
             tags = COALESCE(?4, tags), updated_at = ?5
         WHERE id = ?1",
        params![id, value, author, tags, now],
    )?;
    conn.query_row(
        "SELECT version FROM context_entry WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
}

/// Every entry in a workspace, most recently updated first.
pub fn list_entries(conn: &Connection, workspace_id: &str) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ENTRY_COLUMNS} FROM context_entry WHERE workspace_id = ?1
         ORDER BY updated_at DESC, id DESC"
    ))?;
    let rows = stmt.query_map([workspace_id], entry_from_row)?;
    rows.collect()
}

/// Full-text search over a workspace's entries, best match first.
///
/// Returns `(entry, rank)`. A query FTS5 cannot parse is not an error: a model
/// passing a bare `"` or `AND` should get no hits, not a failed tool call.
pub fn search_entries(
    conn: &Connection,
    workspace_id: &str,
    query: &str,
    limit: u32,
) -> rusqlite::Result<Vec<(Entry, f64)>> {
    let sql = format!(
        "SELECT {} , fts.rank
         FROM context_entry_fts AS fts
         JOIN context_entry AS e ON e.id = fts.rowid
         WHERE context_entry_fts MATCH ?1 AND e.workspace_id = ?2
         ORDER BY fts.rank LIMIT ?3",
        ENTRY_COLUMNS
            .split(", ")
            .map(|c| format!("e.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let run = |text: &str| -> rusqlite::Result<Vec<(Entry, f64)>> {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![text, workspace_id, limit], |row| {
            Ok((entry_from_row(row)?, row.get::<_, f64>(8)?))
        })?;
        rows.collect()
    };
    match run(query) {
        Ok(hits) => Ok(hits),
        // FTS5 raises a plain SQLITE_ERROR for a query it cannot parse, and a
        // model will send one sooner or later. Retry the whole thing as a
        // literal phrase so `AND` finds the word rather than failing the tool.
        Err(rusqlite::Error::SqliteFailure(_, _)) => {
            match run(&format!("\"{}\"", query.replace('"', ""))) {
                Ok(hits) => Ok(hits),
                Err(rusqlite::Error::SqliteFailure(_, _)) => Ok(Vec::new()),
                Err(err) => Err(err),
            }
        }
        Err(err) => Err(err),
    }
}

/// Appends to the log and returns the new sequence number.
pub fn insert_event(conn: &Connection, event: &NewEvent) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO context_event
           (workspace_id, agent_id, kind, key, body, target_agent, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            event.workspace_id,
            event.agent_id,
            event.kind,
            event.key,
            event.body,
            event.target_agent,
            event.created_at,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// A workspace's most recent events, oldest first within the window.
pub fn list_events(
    conn: &Connection,
    workspace_id: &str,
    limit: u32,
) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLUMNS} FROM context_event WHERE workspace_id = ?1
         ORDER BY seq DESC LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![workspace_id, limit], event_from_row)?;
    let mut events: Vec<Event> = rows.collect::<rusqlite::Result<_>>()?;
    events.reverse();
    Ok(events)
}

/// Events after `cursor` that some other agent caused. Newest first, capped.
///
/// An agent is never shown its own events: it already knows what it did, and
/// echoing them back wastes the context budget the digest is rationing.
pub fn events_since(
    conn: &Connection,
    workspace_id: &str,
    after_seq: i64,
    exclude_agent: Option<&str>,
    limit: u32,
) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLUMNS} FROM context_event
         WHERE workspace_id = ?1 AND seq > ?2 AND (?3 IS NULL OR agent_id IS NULL OR agent_id != ?3)
         ORDER BY seq DESC LIMIT ?4"
    ))?;
    let rows = stmt.query_map(
        params![workspace_id, after_seq, exclude_agent, limit],
        event_from_row,
    )?;
    rows.collect()
}

/// The highest sequence number in a workspace, or 0 when the log is empty.
pub fn max_seq(conn: &Connection, workspace_id: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM context_event WHERE workspace_id = ?1",
        [workspace_id],
        |row| row.get(0),
    )
}

/// Unread directed messages for an agent, oldest first.
pub fn unread_messages(conn: &Connection, agent_id: &str) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLUMNS} FROM context_event
         WHERE kind = 'message' AND target_agent = ?1 AND read_at IS NULL
         ORDER BY seq"
    ))?;
    let rows = stmt.query_map([agent_id], event_from_row)?;
    rows.collect()
}

/// How many messages wait unread for each agent that has any.
pub fn unread_counts(conn: &Connection) -> rusqlite::Result<HashMap<String, usize>> {
    let mut stmt = conn.prepare(
        "SELECT target_agent, COUNT(*) FROM context_event
         WHERE kind = 'message' AND read_at IS NULL AND target_agent IS NOT NULL
         GROUP BY target_agent",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    rows.map(|row| row.map(|(agent, count)| (agent, count as usize)))
        .collect()
}

/// The newest message waiting unread for an agent, if any.
pub fn newest_unread(conn: &Connection, agent_id: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT MAX(seq) FROM context_event
         WHERE kind = 'message' AND target_agent = ?1 AND read_at IS NULL",
        [agent_id],
        |row| row.get::<_, Option<i64>>(0),
    )
}

/// The newest message the agent was last woken to read, and the status time
/// it had then.
pub fn woken(conn: &Connection, agent_id: &str) -> rusqlite::Result<(i64, i64)> {
    conn.query_row(
        "SELECT woken_at, woken_idle_at FROM context_cursor WHERE agent_id = ?1",
        [agent_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map(|found| found.unwrap_or((0, 0)))
}

/// Puts the wake mark back where it was, for a nudge that never landed: the
/// only writer allowed to lower it, so the next sweep claims the agent again.
pub fn set_woken(
    conn: &Connection,
    agent_id: &str,
    woken_at: i64,
    woken_idle_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO context_cursor (agent_id, woken_at, woken_idle_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(agent_id) DO UPDATE SET
           woken_at = excluded.woken_at,
           woken_idle_at = excluded.woken_idle_at",
        params![agent_id, woken_at, woken_idle_at],
    )?;
    Ok(())
}

/// Records that the agent has been woken for everything up to `seq`, while it
/// had been idle since `idle_at`.
pub fn mark_woken(
    conn: &Connection,
    agent_id: &str,
    seq: i64,
    idle_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO context_cursor (agent_id, woken_at, woken_idle_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(agent_id) DO UPDATE SET
           woken_at = MAX(woken_at, excluded.woken_at),
           woken_idle_at = MAX(woken_idle_at, excluded.woken_idle_at)",
        params![agent_id, seq, idle_at],
    )?;
    Ok(())
}

/// Marks messages read. Returns how many changed.
pub fn mark_read(conn: &Connection, seqs: &[i64], now: i64) -> rusqlite::Result<usize> {
    let mut changed = 0;
    for seq in seqs {
        changed += conn.execute(
            "UPDATE context_event SET read_at = ?2 WHERE seq = ?1 AND read_at IS NULL",
            params![seq, now],
        )?;
    }
    Ok(changed)
}

/// An agent's delta cursor: `(last_seq, last_delta_at)`, zeroes if it has none.
pub fn cursor(conn: &Connection, agent_id: &str) -> rusqlite::Result<(i64, i64)> {
    conn.query_row(
        "SELECT last_seq, last_delta_at FROM context_cursor WHERE agent_id = ?1",
        [agent_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map(|found| found.unwrap_or((0, 0)))
}

/// Moves an agent's cursor forward. Never backwards: two hooks can race, and
/// the later one must not replay events the earlier one already delivered.
pub fn advance_cursor(
    conn: &Connection,
    agent_id: &str,
    last_seq: i64,
    last_delta_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO context_cursor (agent_id, last_seq, last_delta_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(agent_id) DO UPDATE SET
           last_seq = MAX(last_seq, excluded.last_seq),
           last_delta_at = MAX(last_delta_at, excluded.last_delta_at)",
        params![agent_id, last_seq, last_delta_at],
    )?;
    Ok(())
}

/// A number that grows with every row this connection changes; orders snapshots.
pub fn revision(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT total_changes()", [], |row| row.get(0))
}

/// Removes one event. Returns whether there was one to remove.
pub fn delete_event(conn: &Connection, workspace_id: &str, seq: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "DELETE FROM context_event WHERE workspace_id = ?1 AND seq = ?2",
        params![workspace_id, seq],
    )?;
    Ok(n > 0)
}

/// Removes every event caused by any of `agents`. Returns how many went.
pub fn delete_events_by(
    conn: &Connection,
    workspace_id: &str,
    agents: &[String],
) -> rusqlite::Result<usize> {
    let mut removed = 0;
    for agent in agents {
        removed += conn.execute(
            "DELETE FROM context_event WHERE workspace_id = ?1 AND agent_id = ?2",
            params![workspace_id, agent],
        )?;
    }
    Ok(removed)
}

/// Removes a workspace's whole log. Returns how many events went.
pub fn delete_all_events(conn: &Connection, workspace_id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM context_event WHERE workspace_id = ?1",
        [workspace_id],
    )
}
