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
use crate::platform::slow;

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
                            runtime: pane.runtime,
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

/// Finds the pane, then writes on the blocking pool: a shell that is not
/// reading its input makes the write block.
async fn write(state: &AppState, target: String, bytes: Vec<u8>) -> Result<Sent, WorkspaceError> {
    let id = pane_to_write_to(state, target).await?;
    let pty = state.pty.clone();
    let writing = id.clone();
    // Timed, because a pane that takes no input is the complaint this path is
    // answering, and "it froze" was all anyone could say about it (issue #86).
    let started = std::time::Instant::now();
    let written = tokio::task::spawn_blocking(move || pty.write(&writing, &bytes))
        .await
        .map_err(|err| WorkspaceError::Pty(err.to_string()))?;
    slow::note(&format!("writing to pane {id}"), started, slow::SLOW_WRITE);
    match written {
        Ok(()) => Ok(Sent { pane: id }),
        Err(PtyError::NoSuchPane(_)) => Err(WorkspaceError::PaneNotStarted(id)),
        Err(other) => Err(WorkspaceError::Pty(other.to_string())),
    }
}

/// Which pane to write to, without the database when it can be helped.
///
/// A keystroke arrives with its own pane's id, and the supervisor already
/// knows whether that pane is live - so the lookup that exists for labels,
/// `--pane` arguments and ambiguity is skipped for the one caller that never
/// needs it.
///
/// It used to run on every character (issue #86): `resolve_pane` reads every
/// pane row through Dex's single connection, so each keystroke queued behind
/// every hook from every agent and every query the sweep makes. With five
/// agents hooking at each prompt and tool batch, a pane could take no input
/// for a minute and then take all of it at once.
async fn pane_to_write_to(state: &AppState, target: String) -> Result<String, WorkspaceError> {
    if state.pty.holds(&target) {
        return Ok(target);
    }
    let pane = state
        .db
        .call(move |conn| resolve_pane(conn, &target))
        .await??;
    Ok(pane.id)
}
