//! `pane.*` commands that change the tree — split, create, close, focus,
//! swap, label — plus the workspace commands that edit the whole tree
//! (`workspace.set_layout`, `workspace.cycle_layout`). Tree edits are pure
//! functions in `layout.rs`; these resolve targets, apply, and save.

use dex_protocol::pane::{CreatePaneArgs, Created, LabelPaneArgs};
use dex_protocol::workspace::{
    Layout, PaneArgs, SetLayoutArgs, SplitDir, SplitDirection, SplitPaneArgs, SwapPanesArgs,
    WorkspaceArgs, WorkspaceList,
};
use rusqlite::Connection;

use super::commands::{Outcome, load_list, pane_target};
use super::layout::{self, Closed};
use super::logic;
use super::model::{Pane, WorkspaceError};
use super::store;
use super::targets::{resolve_pane, resolve_workspace, target_workspace};
use crate::app::AppState;
use crate::platform::{clock, ids};

/// A pane, and its workspace's tree and focus as they are right now.
struct Located {
    pane: Pane,
    layout: Layout,
    active: Option<String>,
}

/// A pane about to be created by a split.
struct NewPane {
    id: String,
    cwd: Option<String>,
    label: Option<String>,
    kind: &'static str,
    dir: SplitDir,
    now: i64,
}

/// Resolves a pane and loads its workspace's current tree.
fn locate(conn: &Connection, target: &str) -> rusqlite::Result<Result<Located, WorkspaceError>> {
    let pane = match resolve_pane(conn, target)? {
        Ok(pane) => pane,
        Err(err) => return Ok(Err(err)),
    };
    let Some(workspace) = store::find_workspace(conn, &pane.workspace_id)? else {
        return Ok(Err(WorkspaceError::NoSuchPane(target.to_owned())));
    };
    let pane_ids = pane_ids(conn, &workspace.id)?;
    let Some(layout) = logic::layout_or_default(&workspace.layout_json, &pane_ids) else {
        return Ok(Err(WorkspaceError::NoSuchPane(target.to_owned())));
    };
    Ok(Ok(Located {
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

/// Whether a write hit a uniqueness constraint (here: a duplicate pane label).
fn is_constraint(err: &rusqlite::Error) -> bool {
    matches!(err, rusqlite::Error::SqliteFailure(failure, _)
        if failure.code == rusqlite::ErrorCode::ConstraintViolation)
}

fn label_arg(label: Option<String>) -> Result<Option<String>, WorkspaceError> {
    label
        .map(|label| logic::clean_label(&label).ok_or(WorkspaceError::InvalidLabel))
        .transpose()
}

fn kind_arg(kind: Option<&str>) -> Result<&'static str, WorkspaceError> {
    logic::pane_kind(kind).ok_or_else(|| WorkspaceError::InvalidKind(kind.unwrap_or("").to_owned()))
}

fn split_dir(direction: SplitDirection) -> SplitDir {
    match direction {
        SplitDirection::Right => SplitDir::Horizontal,
        SplitDirection::Down => SplitDir::Vertical,
    }
}

/// Splits `found.pane`, inserting `new` beside it; the new pane takes focus.
fn split_located(conn: &mut Connection, found: Located, new: NewPane) -> Outcome {
    let Some(tree) = layout::split_pane(&found.layout, &found.pane.id, &new.id, new.dir) else {
        return Ok(Err(WorkspaceError::NoSuchPane(found.pane.id)));
    };
    let json = match to_json(&tree) {
        Ok(json) => json,
        Err(err) => return Ok(Err(err)),
    };
    let pane = Pane {
        id: new.id.clone(),
        workspace_id: found.pane.workspace_id.clone(),
        label: new.label.clone(),
        cwd: new.cwd.unwrap_or_else(|| found.pane.cwd.clone()),
        kind: new.kind.into(),
        runtime: found.pane.runtime.clone(),
    };
    let tx = conn.transaction()?;
    match store::insert_pane(&tx, &pane, new.now) {
        Err(err) if is_constraint(&err) => {
            return Ok(Err(WorkspaceError::LabelTaken(
                new.label.unwrap_or_default(),
            )));
        }
        other => other?,
    }
    store::update_layout(&tx, &pane.workspace_id, &json, Some(&new.id), new.now)?;
    tx.commit()?;
    load_list(conn).map(Ok)
}

/// `pane.split`: a new terminal pane beside `pane`, in its folder (or `cwd`)
/// and runtime. The new pane takes focus.
pub async fn split_pane(
    state: &AppState,
    args: SplitPaneArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let new = NewPane {
        id: ids::new_id(),
        cwd: pane_target(kind_arg(args.kind.as_deref())?, args.cwd.as_deref())?,
        label: label_arg(args.label)?,
        kind: kind_arg(args.kind.as_deref())?,
        dir: split_dir(args.direction),
        now: clock::now_millis(),
    };
    state
        .db
        .call(move |conn| -> Outcome {
            let found = match locate(conn, &args.pane)? {
                Ok(found) => found,
                Err(err) => return Ok(Err(err)),
            };
            split_located(conn, found, new)
        })
        .await?
}

/// `pane.create`: a new pane in a workspace — `workspace`, else the one
/// holding `pane`, else the active one — split right from its focused pane.
pub async fn create_pane(
    state: &AppState,
    args: CreatePaneArgs,
) -> Result<Created, WorkspaceError> {
    let new = NewPane {
        id: ids::new_id(),
        cwd: pane_target(kind_arg(args.kind.as_deref())?, args.cwd.as_deref())?,
        label: label_arg(args.label)?,
        kind: kind_arg(args.kind.as_deref())?,
        dir: SplitDir::Horizontal,
        now: clock::now_millis(),
    };
    let new_id = new.id.clone();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Created, WorkspaceError>> {
                let workspace = match target_workspace(
                    conn,
                    args.workspace.as_deref(),
                    args.pane.as_deref(),
                )? {
                    Ok(workspace) => workspace,
                    Err(err) => return Ok(Err(err)),
                };
                let ids = pane_ids(conn, &workspace.id)?;
                let focus = workspace
                    .active_pane
                    .clone()
                    .filter(|id| ids.contains(id))
                    .or_else(|| ids.first().cloned());
                let Some(focus) = focus else {
                    return Ok(Err(WorkspaceError::NoSuchPane(workspace.name)));
                };
                let found = match locate(conn, &focus)? {
                    Ok(found) => found,
                    Err(err) => return Ok(Err(err)),
                };
                Ok(split_located(conn, found, new)?.map(|_| Created { pane: new_id }))
            },
        )
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
            let found = match locate(conn, &args.pane)? {
                Ok(found) => found,
                Err(err) => return Ok(Err(err)),
            };
            let tree = match layout::close_pane(&found.layout, &found.pane.id) {
                Closed::Remaining(tree) => tree,
                Closed::WasLast => return Ok(Err(WorkspaceError::LastPane)),
                Closed::NotFound => return Ok(Err(WorkspaceError::NoSuchPane(args.pane))),
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
            let pane = match resolve_pane(conn, &args.pane)? {
                Ok(pane) => pane,
                Err(err) => return Ok(Err(err)),
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
            let found = match locate(conn, &args.a)? {
                Ok(found) => found,
                Err(err) => return Ok(Err(err)),
            };
            let other = match resolve_pane(conn, &args.b)? {
                Ok(pane) => pane,
                Err(err) => return Ok(Err(err)),
            };
            // None if `b` is in another workspace, or is `a` itself.
            let Some(tree) = layout::swap_panes(&found.layout, &found.pane.id, &other.id) else {
                return Ok(Err(WorkspaceError::NoSuchPane(args.b)));
            };
            let json = match to_json(&tree) {
                Ok(json) => json,
                Err(err) => return Ok(Err(err)),
            };
            store::update_layout(
                conn,
                &found.pane.workspace_id,
                &json,
                Some(&found.pane.id),
                now,
            )?;
            load_list(conn).map(Ok)
        })
        .await?
}

/// `pane.label`: sets (or clears) a pane's label, unique within its workspace.
pub async fn label_pane(
    state: &AppState,
    args: LabelPaneArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let label = label_arg(args.label)?;
    state
        .db
        .call(move |conn| -> Outcome {
            let pane = match resolve_pane(conn, &args.pane)? {
                Ok(pane) => pane,
                Err(err) => return Ok(Err(err)),
            };
            match store::update_label(conn, &pane.id, label.as_deref()) {
                Err(err) if is_constraint(&err) => {
                    return Ok(Err(WorkspaceError::LabelTaken(label.unwrap_or_default())));
                }
                other => {
                    other?;
                }
            }
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
            let workspace = match resolve_workspace(conn, &args.workspace)? {
                Ok(workspace) => workspace,
                Err(err) => return Ok(Err(err)),
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
            let workspace = match resolve_workspace(conn, &args.workspace)? {
                Ok(workspace) => workspace,
                Err(err) => return Ok(Err(err)),
            };
            let ids = pane_ids(conn, &workspace.id)?;
            let current = logic::layout_or_default(&workspace.layout_json, &ids);
            let Some(next) = current.and_then(|tree| layout::next_preset(&tree, &ids)) else {
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
