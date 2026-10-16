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

/// How long the Enter waits behind the text it submits, at the least.
const ENTER_GAP: Duration = Duration::from_millis(50);

/// How long it then waits for the pane to show that it took the text.
pub(super) const ENTER_WAIT: Duration = Duration::from_millis(400);

/// How often to look while waiting.
const LOOK: Duration = Duration::from_millis(10);

/// How long to wait for a pane's shell before writing to it anyway.
///
/// A pane's shell is started by the window holding its terminal, which hears
/// about a new pane a moment after the daemon made it (issue #92), so
/// `dex pane create` followed at once by `dex pane send` can arrive in
/// between. Short, because the other pane with no shell is one whose process
/// has exited, and that caller is waiting to be told so.
pub(super) const SHELL_WAIT: Duration = Duration::from_millis(750);

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
/// The Enter is a second write, behind the text. Claude Code's TUI reads a
/// carriage return that arrives in the same chunk as the text as a pasted
/// newline and leaves the prompt unsent (ARCHITECTURE.md); arriving on its own
/// it is a keypress. Shells behave the same either way.
///
/// Two writes are two chunks only if the pane read the first one in between,
/// and a gap alone cannot make that true: the nudge that wakes an agent is
/// typed at the moment that agent finished a turn, with its transcript to
/// write and its hooks to run, and a prompt left sitting unsent woke nobody
/// (issue #94). So the Enter goes on evidence - the pane has drawn since the
/// text, which it does not do without having read it - and the gap stays as
/// the floor under it, because that is what was measured against a real
/// session.
pub async fn send(state: &AppState, args: SendArgs) -> Result<Sent, WorkspaceError> {
    let pane = pane_to_write_to(state, args.pane).await?;
    let before = state.pty.last_output_at(&pane);
    let sent = write_to(state, pane.clone(), args.text.into_bytes()).await?;
    if !args.enter {
        return Ok(sent);
    }
    tokio::time::sleep(ENTER_GAP).await;
    let said = || state.pty.last_output_at(&pane);
    if !took_the_text(said, before, ENTER_WAIT).await {
        tracing::debug!(%pane, "the pane said nothing about the text; pressing Enter blind");
    }
    write_to(state, pane, b"\r".to_vec()).await
}

/// Waits until the pane has said something since `before`, for up to `within`;
/// false if it never did.
///
/// A terminal echoes what is typed into it and Claude Code redraws its prompt
/// box, so in the ordinary case this returns at once and the Enter is no later
/// than it was before. A pane that says nothing is not held up for ever: the
/// Enter goes anyway, which is what it did on every send until now.
///
/// How strong the evidence is depends on who is echoing. A Windows console
/// echoes as the child reads, so output means the child read. A Unix pty's
/// line discipline echoes whether the child reads or not - but a program that
/// submits a prompt is in raw mode with that echo off, and then the only thing
/// that can draw is the program itself, which is the case this is for.
pub(super) async fn took_the_text(
    said: impl Fn() -> Option<i64>,
    before: Option<i64>,
    within: Duration,
) -> bool {
    until(|| said() != before, within).await
}

/// Waits until `ready`, for up to `within`; false if it never was. It looks
/// before it waits at all, so a caller with nothing to wait for pays nothing.
async fn until(ready: impl Fn() -> bool, within: Duration) -> bool {
    let deadline = std::time::Instant::now() + within;
    loop {
        if ready() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(LOOK).await;
    }
}

/// `pane.send_key`: presses one key in a pane's shell.
pub async fn send_key(state: &AppState, args: SendKeyArgs) -> Result<Sent, WorkspaceError> {
    write(state, args.pane, logic::key_bytes(args.key).to_vec()).await
}

/// Finds the pane, then writes to it.
async fn write(state: &AppState, target: String, bytes: Vec<u8>) -> Result<Sent, WorkspaceError> {
    let id = pane_to_write_to(state, target).await?;
    write_to(state, id, bytes).await
}

/// Writes to a pane already found, on the blocking pool: a shell that is not
/// reading its input makes the write block.
async fn write_to(state: &AppState, id: String, bytes: Vec<u8>) -> Result<Sent, WorkspaceError> {
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
    // The pane is there; its shell can be a moment younger than it is. Waited
    // for here rather than refused, so a pane and a write to it can be two
    // commands in a row (issue #92). A shell that never comes leaves the write
    // itself to fail, as it always did, with the pane named.
    if !until(|| state.pty.holds(&pane.id), SHELL_WAIT).await {
        tracing::debug!(pane = %pane.id, "no shell appeared; writing anyway to say so");
    }
    Ok(pane.id)
}
