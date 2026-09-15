//! One handler per `pane.*` command, plus the workspace commands that edit the
//! whole tree (`workspace.set_layout`, `workspace.cycle_layout`). The tree
//! edits themselves are pure functions in `layout.rs`; these load, apply, save.

use dex_protocol::workspace::{
    Layout, PaneArgs, SetLayoutArgs, SplitDir, SplitDirection, SplitPaneArgs, SwapPanesArgs,
    WorkspaceArgs, WorkspaceList,
};
use rusqlite::Connection;

use super::commands::load_list;
use super::layout::{self, Closed};
use super::logic;
use super::model::{Pane, WorkspaceError};
use super::store;
use crate::app::AppState;
use crate::platform::{clock, ids};

/// What a database closure returns: SQL failures in the outer `Result`, rule
/// violations (unknown pane, last pane, ...) in the inner one.
type Outcome = rusqlite::Result<Result<WorkspaceList, WorkspaceError>>;

/// A pane, and its workspace's tree and focus as they are right now.
struct Located {
    pane: Pane,
    layout: Layout,
    active: Option<String>,
}

/// Finds a pane and its workspace's current tree; `None` if the pane is gone.
fn locate(conn: &Connection, pane_id: &str) -> rusqlite::Result<Option<Located>> {
    let Some(pane) = store::find_pane(conn, pane_id)? else {
        return Ok(None);
    };
    let Some(workspace) = store::find_workspace(conn, &pane.workspace_id)? else {
        return Ok(None);
    };
    let pane_ids = pane_ids(conn, &workspace.id)?;
    let Some(layout) = logic::layout_or_default(&workspace.layout_json, &pane_ids) else {
        return Ok(None);
    };
    Ok(Some(Located {
        pane,
        layout,
        active: workspace.active_pane,
    }))
}

/// A workspace's pane ids in creation order (the order presets arrange them in).
fn pane_ids(conn: &Connection, workspace_id: &str) -> rusqlite::Result<Vec<String>> {
    Ok(store::list_panes(conn, workspace_id)?
        .into_iter()
        .map(|pane| pane.id)
        .collect())
}

fn to_json(tree: &Layout) -> Result<String, WorkspaceError> {
    Ok(serde_json::to_string(tree)?)
}

fn no_such_pane(id: String) -> Outcome {
    Ok(Err(WorkspaceError::NoSuchPane(id)))
}

/// `pane.split`: a new terminal pane beside `pane`, in the same folder and
/// runtime. The new pane takes focus.
pub async fn split_pane(
    state: &AppState,
    args: SplitPaneArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let new_id = ids::new_id();
    let now = clock::now_millis();
    let dir = match args.direction {
        SplitDirection::Right => SplitDir::Horizontal,
        SplitDirection::Down => SplitDir::Vertical,
    };
    state
        .db
        .call(move |conn| -> Outcome {
            let Some(found) = locate(conn, &args.pane)? else {
                return no_such_pane(args.pane);
            };
            let Some(tree) = layout::split_pane(&found.layout, &found.pane.id, &new_id, dir) else {
                return no_such_pane(args.pane);
            };
            let json = match to_json(&tree) {
                Ok(json) => json,
                Err(err) => return Ok(Err(err)),
            };
            let pane = Pane {
                id: new_id.clone(),
                workspace_id: found.pane.workspace_id.clone(),
                label: None,
                cwd: found.pane.cwd.clone(),
                kind: "terminal".into(),
                runtime: found.pane.runtime.clone(),
            };
            let tx = conn.transaction()?;
            store::insert_pane(&tx, &pane, now)?;
            store::update_layout(&tx, &pane.workspace_id, &json, Some(&new_id), now)?;
            tx.commit()?;
            load_list(conn).map(Ok)
        })
        .await?
}

/// `pane.close`: removes a pane; its sibling takes the parent split's place.
/// A workspace keeps at least one pane. If the closed pane had focus, focus
/// moves to the first pane left.
pub async fn close_pane(state: &AppState, args: PaneArgs) -> Result<WorkspaceList, WorkspaceError> {
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| -> Outcome {
            let Some(found) = locate(conn, &args.pane)? else {
                return no_such_pane(args.pane);
            };
            let tree = match layout::close_pane(&found.layout, &found.pane.id) {
                Closed::Remaining(tree) => tree,
                Closed::WasLast => return Ok(Err(WorkspaceError::LastPane)),
                Closed::NotFound => return no_such_pane(args.pane),
            };
            let focus = if found.active.as_deref() == Some(found.pane.id.as_str()) {
                layout::leaves(&tree).into_iter().next()
            } else {
                found.active
            };
            let json = match to_json(&tree) {
                Ok(json) => json,
                Err(err) => return Ok(Err(err)),
            };
            let tx = conn.transaction()?;
            store::delete_pane(&tx, &found.pane.id)?;
            store::update_layout(&tx, &found.pane.workspace_id, &json, focus.as_deref(), now)?;
            tx.commit()?;
            load_list(conn).map(Ok)
        })
        .await?
}

/// `pane.focus`: records which pane of its workspace has focus.
pub async fn focus_pane(state: &AppState, args: PaneArgs) -> Result<WorkspaceList, WorkspaceError> {
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| -> Outcome {
            let Some(pane) = store::find_pane(conn, &args.pane)? else {
                return no_such_pane(args.pane);
            };
            store::update_active_pane(conn, &pane.workspace_id, &pane.id, now)?;
            load_list(conn).map(Ok)
        })
        .await?
}

/// `pane.swap`: two panes of one workspace trade places; `a` keeps focus.
pub async fn swap_panes(
    state: &AppState,
    args: SwapPanesArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| -> Outcome {
            let Some(found) = locate(conn, &args.a)? else {
                return no_such_pane(args.a);
            };
            // Fails if `b` is missing, in another workspace, or `a` itself.
            let Some(tree) = layout::swap_panes(&found.layout, &args.a, &args.b) else {
                return no_such_pane(args.b);
            };
            let json = match to_json(&tree) {
                Ok(json) => json,
                Err(err) => return Ok(Err(err)),
            };
            store::update_layout(conn, &found.pane.workspace_id, &json, Some(&args.a), now)?;
            load_list(conn).map(Ok)
        })
        .await?
}

/// `workspace.set_layout`: replaces the tree (after a divider drag). The new
/// tree must show exactly the workspace's panes; ratios are clamped.
pub async fn set_layout(
    state: &AppState,
    args: SetLayoutArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let tree = layout::clamp_all(&args.layout);
    let json = to_json(&tree)?;
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| -> Outcome {
            let Some(workspace) = store::find_workspace(conn, &args.workspace)? else {
                return Ok(Err(WorkspaceError::NoSuchWorkspace(args.workspace)));
            };
            let mut shown = layout::leaves(&tree);
            let mut expected = pane_ids(conn, &workspace.id)?;
            shown.sort();
            expected.sort();
            if shown != expected {
                return Ok(Err(WorkspaceError::LayoutMismatch));
            }
            store::update_layout(
                conn,
                &workspace.id,
                &json,
                workspace.active_pane.as_deref(),
                now,
            )?;
            load_list(conn).map(Ok)
        })
        .await?
}

/// `workspace.cycle_layout`: rebuilds the tree as the next preset, from the
/// panes in creation order (PRD §7.2).
pub async fn cycle_layout(
    state: &AppState,
    args: WorkspaceArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| -> Outcome {
            let Some(workspace) = store::find_workspace(conn, &args.workspace)? else {
                return Ok(Err(WorkspaceError::NoSuchWorkspace(args.workspace)));
            };
            let pane_ids = pane_ids(conn, &workspace.id)?;
            let current = logic::layout_or_default(&workspace.layout_json, &pane_ids);
            let Some(next) = current.and_then(|tree| layout::next_preset(&tree, &pane_ids)) else {
                // No panes: nothing to arrange.
                return load_list(conn).map(Ok);
            };
            let json = match to_json(&next) {
                Ok(json) => json,
                Err(err) => return Ok(Err(err)),
            };
            store::update_layout(
                conn,
                &workspace.id,
                &json,
                workspace.active_pane.as_deref(),
                now,
            )?;
            load_list(conn).map(Ok)
        })
        .await?
}
