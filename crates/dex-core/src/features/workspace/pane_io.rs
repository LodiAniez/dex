//! Pane commands for the CLI that read state or reach into a pane's shell:
//! `pane.list`, `pane.send`, `pane.send_key` (docs/prd.md §11).

use std::time::Duration;

use dex_protocol::pane::{ListPanesArgs, PaneList, PaneSummary, SendArgs, SendKeyArgs, Sent};

use super::logic;
use super::model::WorkspaceError;
use super::store;
use super::targets::{resolve_pane, target_workspace};
use crate::app::AppState;
use crate::platform::pty::PtyError;

/// How long the Enter waits behind the text it submits.
const ENTER_GAP: Duration = Duration::from_millis(50);

/// `pane.list`: panes of one workspace (by name/id, or the one holding a
/// given pane), or of every workspace.
pub async fn list_panes(state: &AppState, args: ListPanesArgs) -> Result<PaneList, WorkspaceError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<PaneList, WorkspaceError>> {
                let scope = if args.workspace.is_some() || args.pane.is_some() {
                    match target_workspace(conn, args.workspace.as_deref(), args.pane.as_deref())? {
                        Ok(workspace) => Some(workspace.id),
                        Err(err) => return Ok(Err(err)),
                    }
                } else {
                    None
                };
                let active = store::find_active_workspace(conn)?;
                let mut panes = Vec::new();
                for workspace in store::list_workspaces(conn)? {
                    if scope.as_ref().is_some_and(|id| id != &workspace.id) {
                        continue;
                    }
                    for pane in store::list_panes(conn, &workspace.id)? {
                        panes.push(PaneSummary {
                            focused: workspace.active_pane.as_deref() == Some(pane.id.as_str()),
                            in_active_workspace: active.as_deref() == Some(workspace.id.as_str()),
                            workspace: workspace.name.clone(),
                            workspace_id: workspace.id.clone(),
                            id: pane.id,
                            label: pane.label,
                            cwd: pane.cwd,
                            kind: pane.kind,
                        });
                    }
                }
                Ok(Ok(PaneList { panes }))
            },
        )
        .await?
}

/// `pane.send`: types text into a pane's shell, then Enter if asked.
///
/// The Enter is a second write, a moment later. Claude Code's TUI reads a
/// carriage return that arrives in the same chunk as the text as a pasted
/// newline and leaves the prompt unsent (ARCHITECTURE.md); arriving on its own
/// it is a keypress. Shells behave the same either way.
pub async fn send(state: &AppState, args: SendArgs) -> Result<Sent, WorkspaceError> {
    let sent = write(state, args.pane, args.text.into_bytes()).await?;
    if !args.enter {
        return Ok(sent);
    }
    tokio::time::sleep(ENTER_GAP).await;
    write(state, sent.pane, b"\r".to_vec()).await
}

/// `pane.send_key`: presses one key in a pane's shell.
pub async fn send_key(state: &AppState, args: SendKeyArgs) -> Result<Sent, WorkspaceError> {
    write(state, args.pane, logic::key_bytes(args.key).to_vec()).await
}

/// Resolves the target, then writes on the blocking pool: a shell that is not
/// reading its input makes the write block.
async fn write(state: &AppState, target: String, bytes: Vec<u8>) -> Result<Sent, WorkspaceError> {
    let pane = state
        .db
        .call(move |conn| resolve_pane(conn, &target))
        .await??;
    let pty = state.pty.clone();
    let id = pane.id.clone();
    let written = tokio::task::spawn_blocking(move || pty.write(&id, &bytes))
        .await
        .map_err(|err| WorkspaceError::Pty(err.to_string()))?;
    match written {
        Ok(()) => Ok(Sent { pane: pane.id }),
        Err(PtyError::NoSuchPane(_)) => Err(WorkspaceError::PaneNotStarted(pane.id)),
        Err(other) => Err(WorkspaceError::Pty(other.to_string())),
    }
}
