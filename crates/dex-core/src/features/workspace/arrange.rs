//! Keeping an arrangement as panes arrive. A workspace laid out as one of the
//! presets that grow well - one large pane and the rest beside or below it, or
//! a grid - stays that way when a spawn adds a pane, instead of the new pane
//! splitting whatever it was split from (issue #31). A layout the owner made by
//! hand is theirs, and is left as it is.

#[cfg(test)]
use dex_protocol::workspace::Layout;
use dex_protocol::workspace::WorkspaceList;

use super::commands::load_list;
use super::layout::{self, Preset};
use super::model::WorkspaceError;
use super::{logic, store};
use crate::app::AppState;
use crate::platform::clock;

/// The preset a workspace is arranged as right now, if it is exactly one that
/// grows well; `None` for a hand-made layout, a single pane, or an even row or
/// column, none of which a spawn should try to keep.
pub async fn preset_in_use(
    state: &AppState,
    workspace_id: &str,
) -> Result<Option<Preset>, WorkspaceError> {
    let workspace_id = workspace_id.to_owned();
    let found = state
        .db
        .call(
            move |conn| -> rusqlite::Result<Option<(String, Vec<String>)>> {
                let Some(workspace) = store::find_workspace(conn, &workspace_id)? else {
                    return Ok(None);
                };
                let ids = store::list_panes(conn, &workspace_id)?
                    .into_iter()
                    .map(|pane| pane.id)
                    .collect();
                Ok(Some((workspace.layout_json, ids)))
            },
        )
        .await?;
    let Some((json, ids)) = found else {
        return Ok(None);
    };
    if ids.len() < 2 {
        return Ok(None);
    }
    let current = logic::layout_or_default(&json, &ids);
    Ok(current
        .and_then(|tree| layout::preset_of(&tree, &ids))
        .filter(|preset| preset.grows_well()))
}

/// Rebuilds the workspace's tree as `preset`, from its panes in creation order.
pub async fn arrange(
    state: &AppState,
    workspace_id: &str,
    preset: Preset,
) -> Result<WorkspaceList, WorkspaceError> {
    let workspace_id = workspace_id.to_owned();
    let now = clock::now_millis();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<WorkspaceList, WorkspaceError>> {
                let Some(workspace) = store::find_workspace(conn, &workspace_id)? else {
                    return Ok(Err(WorkspaceError::NoSuchWorkspace(workspace_id)));
                };
                let ids: Vec<String> = store::list_panes(conn, &workspace_id)?
                    .into_iter()
                    .map(|pane| pane.id)
                    .collect();
                let Some(tree) = layout::preset(preset, &ids) else {
                    return load_list(conn).map(Ok);
                };
                let json = match serde_json::to_string(&tree) {
                    Ok(json) => json,
                    Err(err) => return Ok(Err(err.into())),
                };
                store::update_layout(
                    conn,
                    &workspace_id,
                    &json,
                    workspace.active_pane.as_deref(),
                    now,
                )?;
                load_list(conn).map(Ok)
            },
        )
        .await?
}

/// `preset` applied to `panes`: what an arrangement should look like, for
/// tests in other slices to say so.
#[cfg(test)]
pub fn preset_layout(preset: Preset, panes: &[String]) -> Option<Layout> {
    layout::preset(preset, panes)
}
