//! Every SQL statement touching `workspace`, `pane`, and `app_state`. No other
//! module queries these tables (docs/conventions.md §1.3).

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::model::{Pane, Workspace};

/// `app_state` key holding the id of the workspace on screen.
const ACTIVE_WORKSPACE: &str = "active_workspace";

const WORKSPACE_COLUMNS: &str = "id, name, root_path, color, sort_index, layout_json, active_pane";

fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        root_path: row.get(2)?,
        color: row.get(3)?,
        sort_index: row.get(4)?,
        layout_json: row.get(5)?,
        active_pane: row.get(6)?,
    })
}

/// Inserts a workspace; `now` becomes both timestamps.
pub fn insert_workspace(conn: &Connection, ws: &Workspace, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO workspace
           (id, name, root_path, color, sort_index, layout_json, active_pane, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            ws.id,
            ws.name,
            ws.root_path,
            ws.color,
            ws.sort_index,
            ws.layout_json,
            ws.active_pane,
            now
        ],
    )?;
    Ok(())
}

/// Every workspace, in sidebar order.
pub fn list_workspaces(conn: &Connection) -> rusqlite::Result<Vec<Workspace>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {WORKSPACE_COLUMNS} FROM workspace ORDER BY sort_index, created_at"
    ))?;
    let rows = stmt.query_map([], workspace_from_row)?;
    rows.collect()
}

/// One workspace, if it exists.
pub fn find_workspace(conn: &Connection, id: &str) -> rusqlite::Result<Option<Workspace>> {
    conn.query_row(
        &format!("SELECT {WORKSPACE_COLUMNS} FROM workspace WHERE id = ?1"),
        [id],
        workspace_from_row,
    )
    .optional()
}

/// Renames a workspace. False if there is no such workspace.
pub fn update_name(conn: &Connection, id: &str, name: &str, now: i64) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE workspace SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now],
    )?;
    Ok(changed > 0)
}

/// Sets or clears a workspace's color. False if there is no such workspace.
pub fn update_color(
    conn: &Connection,
    id: &str,
    color: Option<&str>,
    now: i64,
) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE workspace SET color = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, color, now],
    )?;
    Ok(changed > 0)
}

/// Applies new sidebar positions, all or nothing.
pub fn update_sort_indexes(
    conn: &mut Connection,
    order: &[(String, i64)],
    now: i64,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    for (id, index) in order {
        tx.execute(
            "UPDATE workspace SET sort_index = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, index, now],
        )?;
    }
    tx.commit()
}

/// Deletes a workspace; its panes go with it (`ON DELETE CASCADE`).
/// False if there is no such workspace.
pub fn delete_workspace(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM workspace WHERE id = ?1", [id])? > 0)
}

/// The sidebar position after the last workspace.
pub fn next_sort_index(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(sort_index) + 1, 0) FROM workspace",
        [],
        |row| row.get(0),
    )
}

/// Inserts a pane.
pub fn insert_pane(conn: &Connection, pane: &Pane, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO pane (id, workspace_id, label, cwd, kind, runtime, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            pane.id,
            pane.workspace_id,
            pane.label,
            pane.cwd,
            pane.kind,
            pane.runtime,
            now
        ],
    )?;
    Ok(())
}

const PANE_COLUMNS: &str = "id, workspace_id, label, cwd, kind, runtime";

fn pane_from_row(row: &Row<'_>) -> rusqlite::Result<Pane> {
    Ok(Pane {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        label: row.get(2)?,
        cwd: row.get(3)?,
        kind: row.get(4)?,
        runtime: row.get(5)?,
    })
}

/// A workspace's panes, oldest first.
pub fn list_panes(conn: &Connection, workspace_id: &str) -> rusqlite::Result<Vec<Pane>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PANE_COLUMNS} FROM pane WHERE workspace_id = ?1 ORDER BY created_at, rowid"
    ))?;
    let rows = stmt.query_map([workspace_id], pane_from_row)?;
    rows.collect()
}

/// Deletes a pane. False if there is no such pane.
pub fn delete_pane(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM pane WHERE id = ?1", [id])? > 0)
}

/// Stores a workspace's layout tree and focused pane together.
pub fn update_layout(
    conn: &Connection,
    workspace_id: &str,
    layout_json: &str,
    active_pane: Option<&str>,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE workspace SET layout_json = ?2, active_pane = ?3, updated_at = ?4 WHERE id = ?1",
        params![workspace_id, layout_json, active_pane, now],
    )?;
    Ok(())
}

/// Records which pane of a workspace has focus.
pub fn update_active_pane(
    conn: &Connection,
    workspace_id: &str,
    pane_id: &str,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE workspace SET active_pane = ?2, updated_at = ?3 WHERE id = ?1",
        params![workspace_id, pane_id, now],
    )?;
    Ok(())
}

/// The workspace a pane belongs to, if the pane exists. Part of the slice's
/// public face: other slices ask this instead of querying `pane`.
pub fn find_pane_workspace(conn: &Connection, pane_id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT workspace_id FROM pane WHERE id = ?1",
        [pane_id],
        |row| row.get(0),
    )
    .optional()
}

/// A number that grows with every row this connection changes. All writes go
/// through the daemon's single connection, so it orders snapshots.
pub fn revision(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT total_changes()", [], |row| row.get(0))
}

/// Every pane of every workspace, for resolving `--target` labels.
pub fn list_all_panes(conn: &Connection) -> rusqlite::Result<Vec<Pane>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PANE_COLUMNS} FROM pane ORDER BY created_at, rowid"
    ))?;
    let rows = stmt.query_map([], pane_from_row)?;
    rows.collect()
}

/// Sets or clears a pane's label. A label another pane of the workspace
/// already has fails with a constraint violation (`pane_label_unique`).
pub fn update_label(
    conn: &Connection,
    pane_id: &str,
    label: Option<&str>,
) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE pane SET label = ?2 WHERE id = ?1",
        params![pane_id, label],
    )?;
    Ok(changed > 0)
}

/// The recorded active workspace id. May name a workspace deleted since.
pub fn find_active_workspace(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM app_state WHERE key = ?1",
        [ACTIVE_WORKSPACE],
        |row| row.get(0),
    )
    .optional()
}

/// Records the active workspace.
pub fn update_active_workspace(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_state (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![ACTIVE_WORKSPACE, id],
    )?;
    Ok(())
}
