//! `pane.move`: a pane dragged by its header and dropped on another pane of
//! its workspace (`docking.rs` has the rule). The moved pane takes focus.

use dex_protocol::workspace::{MovePaneArgs, WorkspaceList};

use super::commands::{Outcome, load_list};
use super::docking;
use super::model::WorkspaceError;
use super::targets::resolve_pane;
use super::{logic, store};
use crate::app::AppState;
use crate::platform::clock;

pub async fn move_pane(
    state: &AppState,
    args: MovePaneArgs,
) -> Result<WorkspaceList, WorkspaceError> {
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| -> Outcome {
            let moving = match resolve_pane(conn, &args.pane)? {
                Ok(pane) => pane,
                Err(err) => return Ok(Err(err)),
            };
            let target = match resolve_pane(conn, &args.target)? {
                Ok(pane) => pane,
                Err(err) => return Ok(Err(err)),
            };
            let Some(workspace) = store::find_workspace(conn, &moving.workspace_id)? else {
                return Ok(Err(WorkspaceError::NoSuchPane(args.pane)));
            };
            let ids: Vec<String> = store::list_panes(conn, &workspace.id)?
                .into_iter()
                .map(|pane| pane.id)
                .collect();
            let current = logic::layout_or_default(&workspace.layout_json, &ids);
            // None for a pane in another workspace, or the pane itself.
            let Some(tree) = current
                .and_then(|tree| docking::move_pane(&tree, &moving.id, &target.id, args.side))
            else {
                return Ok(Err(WorkspaceError::NoSuchPane(args.target)));
            };
            let json = match serde_json::to_string(&tree) {
                Ok(json) => json,
                Err(err) => return Ok(Err(err.into())),
            };
            store::update_layout(conn, &workspace.id, &json, Some(&moving.id), now)?;
            load_list(conn).map(Ok)
        })
        .await?
}
