//! Where a spawned agent's pane goes (issue #31).
//!
//! Every spawn used to split the caller's pane: a lead that spawned six agents
//! was sliced six times, and the six landed wherever the successive splits put
//! them. Now the first child splits the caller the way it asked (right, or
//! down); every later child splits the *newest sibling*, across that
//! direction - so the agents form a strip beside (or below) the lead, whose
//! pane keeps its size. And a workspace the owner has arranged as a preset that
//! grows well - main pane and the rest, or a grid - is re-arranged as that
//! preset with the new pane in it, rather than split at all. A layout made by
//! hand is left alone: the new pane joins the strip.

use dex_protocol::workspace::{SplitDirection, SplitPaneArgs, WorkspaceList};

use super::model::AgentError;
use super::store;
use crate::app::AppState;
use crate::features::workspace;

/// Which pane the new one is split from, and which way. `siblings` are the
/// panes of the caller's living children in the order they were hired.
pub fn spawn_anchor(
    caller_pane: &str,
    siblings: &[String],
    direction: Option<&str>,
) -> (String, SplitDirection) {
    let first = match direction {
        Some("down") => SplitDirection::Down,
        _ => SplitDirection::Right,
    };
    match siblings.last() {
        None => (caller_pane.to_owned(), first),
        // Across the first direction: a strip beside the lead, not a slicing of it.
        Some(newest) => (
            newest.clone(),
            match first {
                SplitDirection::Right => SplitDirection::Down,
                SplitDirection::Down => SplitDirection::Right,
            },
        ),
    }
}

/// Makes the child's pane where it belongs, and keeps the workspace's
/// arrangement if it had one worth keeping.
pub async fn split_for(
    state: &AppState,
    workspace_id: &str,
    caller_pane: &str,
    caller_agent: Option<&str>,
    direction: Option<&str>,
    cwd: String,
    label: Option<String>,
) -> Result<WorkspaceList, AgentError> {
    let kept = workspace::preset_in_use(state, workspace_id).await?;
    let (ws, pane, parent) = (
        workspace_id.to_owned(),
        caller_pane.to_owned(),
        caller_agent.map(str::to_owned),
    );
    let siblings = state
        .db
        .call(move |conn| store::live_sibling_panes(conn, &ws, parent.as_deref(), &pane))
        .await?;
    let (anchor, dir) = spawn_anchor(caller_pane, &siblings, direction);
    let list = workspace::split_pane(
        state,
        SplitPaneArgs {
            pane: anchor,
            direction: dir,
            cwd: Some(cwd),
            label,
            kind: None,
        },
    )
    .await?;
    match kept {
        Some(preset) => Ok(workspace::arrange(state, workspace_id, preset).await?),
        None => Ok(list),
    }
}
