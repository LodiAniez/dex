//! Target resolution (PRD §6.3): which pane or workspace an argument names.
//! An exact id wins; otherwise an exact label or name; otherwise a
//! case-insensitive one. More than one match is an error listing them all.

use rusqlite::Connection;

use super::logic::{self, Resolved};
use super::model::{Pane, Workspace, WorkspaceError};
use super::store;

/// The workspace a target names: its id, or its name.
pub(super) fn resolve_workspace(
    conn: &Connection,
    target: &str,
) -> rusqlite::Result<Result<Workspace, WorkspaceError>> {
    let workspaces = store::list_workspaces(conn)?;
    let candidates: Vec<(&str, Option<&str>)> = workspaces
        .iter()
        .map(|ws| (ws.id.as_str(), Some(ws.name.as_str())))
        .collect();
    Ok(match logic::resolve(target, &candidates) {
        Resolved::One(index) => Ok(workspaces[index].clone()),
        Resolved::Nothing => Err(WorkspaceError::NoSuchWorkspace(target.to_owned())),
        Resolved::Many(indexes) => Err(WorkspaceError::AmbiguousTarget {
            target: target.to_owned(),
            candidates: indexes
                .iter()
                .map(|&i| format!("{} ({})", workspaces[i].name, workspaces[i].id))
                .collect(),
        }),
    })
}

/// The pane a target names: its id, or its label.
pub(super) fn resolve_pane(
    conn: &Connection,
    target: &str,
) -> rusqlite::Result<Result<Pane, WorkspaceError>> {
    let panes = store::list_all_panes(conn)?;
    let candidates: Vec<(&str, Option<&str>)> = panes
        .iter()
        .map(|pane| (pane.id.as_str(), pane.label.as_deref()))
        .collect();
    Ok(match logic::resolve(target, &candidates) {
        Resolved::One(index) => Ok(panes[index].clone()),
        Resolved::Nothing => Err(WorkspaceError::NoSuchPane(target.to_owned())),
        Resolved::Many(indexes) => Err(WorkspaceError::AmbiguousTarget {
            target: target.to_owned(),
            candidates: indexes.iter().map(|&i| describe(&panes[i])).collect(),
        }),
    })
}

fn describe(pane: &Pane) -> String {
    match &pane.label {
        Some(label) => format!("{label} ({})", pane.id),
        None => pane.id.clone(),
    }
}

/// The workspace a command means: `workspace` if given, else the one holding
/// `pane`, else the active one.
pub(super) fn target_workspace(
    conn: &Connection,
    workspace: Option<&str>,
    pane: Option<&str>,
) -> rusqlite::Result<Result<Workspace, WorkspaceError>> {
    if let Some(target) = workspace {
        return resolve_workspace(conn, target);
    }
    let id = match pane {
        Some(target) => match resolve_pane(conn, target)? {
            Ok(pane) => Some(pane.workspace_id),
            Err(err) => return Ok(Err(err)),
        },
        None => store::find_active_workspace(conn)?,
    };
    let found = match id {
        Some(id) => store::find_workspace(conn, &id)?,
        None => None,
    };
    // A stale active id (workspace deleted since) falls back to the first workspace.
    let found = match found {
        Some(ws) => Some(ws),
        None => store::list_workspaces(conn)?.into_iter().next(),
    };
    Ok(found.ok_or_else(|| WorkspaceError::NoSuchWorkspace("(none exist)".into())))
}
