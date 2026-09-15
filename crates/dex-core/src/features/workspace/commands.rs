//! One handler per `workspace.*` command. Every handler answers with the full
//! `WorkspaceList`, so clients replace their state instead of patching it.

use std::path::PathBuf;

use dex_protocol::workspace::{
    CreateWorkspaceArgs, Layout, PaneView, RecolorWorkspaceArgs, RenameWorkspaceArgs,
    ReorderWorkspacesArgs, WorkspaceArgs, WorkspaceList, WorkspaceView,
};
use rusqlite::Connection;

use super::logic;
use super::model::{Pane, Workspace, WorkspaceError};
use super::store;
use crate::app::AppState;
use crate::platform::{clock, ids, paths};

/// `workspace.list`: every workspace with its panes, and which one is active.
pub async fn list(state: &AppState) -> Result<WorkspaceList, WorkspaceError> {
    Ok(state.db.call(load_list).await?)
}

/// `workspace.create`: a new workspace with one terminal pane at its root.
/// The new workspace becomes the active one.
pub async fn create(
    state: &AppState,
    args: CreateWorkspaceArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let root_path = resolve_root(args.root_path)?;
    let name = args
        .name
        .map(|name| logic::clean_name(&name).ok_or(WorkspaceError::InvalidName))
        .transpose()?;
    let color = args.color.map(parse_color).transpose()?;
    let workspace_id = ids::new_id();
    let pane_id = ids::new_id();
    let layout_json = serde_json::to_string(&Layout::Leaf {
        pane_id: pane_id.clone(),
    })?;
    let now = clock::now_millis();

    let list = state
        .db
        .call(move |conn| {
            let tx = conn.transaction()?;
            let existing = store::list_workspaces(&tx)?;
            let taken: Vec<String> = existing.iter().map(|ws| ws.name.clone()).collect();
            let workspace = Workspace {
                id: workspace_id.clone(),
                name: name.unwrap_or_else(|| logic::default_name(&taken)),
                color: Some(color.unwrap_or_else(|| logic::default_color(existing.len()))),
                sort_index: store::next_sort_index(&tx)?,
                root_path: root_path.clone(),
                layout_json,
                active_pane: Some(pane_id.clone()),
            };
            let pane = Pane {
                id: pane_id,
                workspace_id: workspace_id.clone(),
                label: None,
                cwd: root_path,
                kind: "terminal".into(),
                runtime: "windows".into(),
            };
            store::insert_workspace(&tx, &workspace, now)?;
            store::insert_pane(&tx, &pane, now)?;
            store::update_active_workspace(&tx, &workspace_id)?;
            tx.commit()?;
            load_list(conn)
        })
        .await?;
    Ok(list)
}

/// `workspace.rename`.
pub async fn rename(
    state: &AppState,
    args: RenameWorkspaceArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let name = logic::clean_name(&args.name).ok_or(WorkspaceError::InvalidName)?;
    let now = clock::now_millis();
    update_one(state, args.workspace, move |conn, id| {
        store::update_name(conn, id, &name, now)
    })
    .await
}

/// `workspace.recolor`: sets a `#rrggbb` color, or clears it with `None`.
pub async fn recolor(
    state: &AppState,
    args: RecolorWorkspaceArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let color = args.color.map(parse_color).transpose()?;
    let now = clock::now_millis();
    update_one(state, args.workspace, move |conn, id| {
        store::update_color(conn, id, color.as_deref(), now)
    })
    .await
}

/// `workspace.switch`: makes a workspace the active one.
pub async fn switch(
    state: &AppState,
    args: WorkspaceArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    update_one(state, args.workspace, |conn, id| {
        if store::find_workspace(conn, id)?.is_none() {
            return Ok(false);
        }
        store::update_active_workspace(conn, id)?;
        Ok(true)
    })
    .await
}

/// `workspace.delete`: removes a workspace and its panes. If it was active, the
/// first remaining workspace becomes active.
pub async fn delete(
    state: &AppState,
    args: WorkspaceArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    update_one(state, args.workspace, |conn, id| {
        store::delete_workspace(conn, id)
    })
    .await
}

/// `workspace.reorder`: new sidebar order, naming every workspace exactly once.
pub async fn reorder(
    state: &AppState,
    args: ReorderWorkspacesArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let now = clock::now_millis();
    let outcome = state
        .db
        .call(move |conn| {
            let existing: Vec<String> = store::list_workspaces(conn)?
                .into_iter()
                .map(|ws| ws.id)
                .collect();
            let Some(order) = logic::reorder(&existing, &args.order) else {
                return Ok(None);
            };
            store::update_sort_indexes(conn, &order, now)?;
            load_list(conn).map(Some)
        })
        .await?;
    outcome.ok_or(WorkspaceError::InvalidOrder)
}

/// Runs a single-workspace update, then reloads the list. The update returns
/// false when no row matched, which becomes `NoSuchWorkspace`.
async fn update_one<F>(
    state: &AppState,
    id: String,
    update: F,
) -> Result<WorkspaceList, WorkspaceError>
where
    F: FnOnce(&mut Connection, &str) -> rusqlite::Result<bool> + Send + 'static,
{
    let key = id.clone();
    let outcome = state
        .db
        .call(move |conn| {
            if !update(conn, &key)? {
                return Ok(None);
            }
            load_list(conn).map(Some)
        })
        .await?;
    outcome.ok_or(WorkspaceError::NoSuchWorkspace(id))
}

/// Reads the whole workspace state and shapes it for the wire.
fn load_list(conn: &mut Connection) -> rusqlite::Result<WorkspaceList> {
    let mut workspaces = Vec::new();
    for workspace in store::list_workspaces(conn)? {
        let panes = store::list_panes(conn, &workspace.id)?;
        workspaces.push(view(workspace, panes));
    }
    // The recorded id can name a workspace deleted since; fall back to the first.
    let active = store::find_active_workspace(conn)?
        .filter(|id| workspaces.iter().any(|ws| &ws.id == id))
        .or_else(|| workspaces.first().map(|ws| ws.id.clone()));
    Ok(WorkspaceList { workspaces, active })
}

fn view(workspace: Workspace, panes: Vec<Pane>) -> WorkspaceView {
    let pane_ids: Vec<String> = panes.iter().map(|pane| pane.id.clone()).collect();
    WorkspaceView {
        layout: logic::layout_or_default(&workspace.layout_json, &pane_ids),
        id: workspace.id,
        name: workspace.name,
        color: workspace.color,
        root_path: workspace.root_path,
        sort_index: u32::try_from(workspace.sort_index).unwrap_or(u32::MAX),
        active_pane: workspace.active_pane,
        panes: panes
            .into_iter()
            .map(|pane| PaneView {
                id: pane.id,
                label: pane.label,
                cwd: pane.cwd,
                kind: pane.kind,
                runtime: pane.runtime,
            })
            .collect(),
    }
}

fn parse_color(input: String) -> Result<String, WorkspaceError> {
    logic::normalize_color(&input).ok_or(WorkspaceError::InvalidColor(input))
}

/// The workspace root as stored: the requested directory, or the home directory.
fn resolve_root(requested: Option<String>) -> Result<String, WorkspaceError> {
    let path = match requested {
        Some(path) => PathBuf::from(path.trim()),
        None => {
            paths::home_dir().ok_or_else(|| WorkspaceError::InvalidRoot("%USERPROFILE%".into()))?
        }
    };
    let stored = paths::normalize(&path);
    if !path.is_absolute() || !path.is_dir() {
        return Err(WorkspaceError::InvalidRoot(stored));
    }
    Ok(stored)
}
