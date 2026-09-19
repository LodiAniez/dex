//! Which terminal Dex opens: PowerShell (or the configured shell) on Windows,
//! or a WSL distro's shell (`platform/wsl.rs`). One choice for the whole app,
//! kept in `app_state`: every new pane opens there - a new workspace's, a
//! split, a spawned agent's - and when Dex starts, every pane does. A pane
//! records the terminal its shell starts in as its `runtime`, so one already
//! running keeps going where it is when the choice changes.

use std::collections::HashMap;
use std::path::Path;

use dex_protocol::pane::{TerminalArgs, TerminalView};

use super::model::WorkspaceError;
use super::store;
use crate::app::AppState;
use crate::platform::proctree;
use crate::platform::wsl::{self, Runtime};

/// Checks a terminal against the distros installed, and names the distro as
/// `wsl.exe` does (it matches names regardless of case). Pure: `installed` is
/// what `wsl.exe` listed.
pub(super) fn check(text: &str, installed: &[String]) -> Result<String, WorkspaceError> {
    match Runtime::parse(text).map_err(WorkspaceError::InvalidRuntime)? {
        Runtime::Windows => Ok(Runtime::Windows.to_string()),
        Runtime::Wsl(distro) => match installed
            .iter()
            .find(|name| name.eq_ignore_ascii_case(&distro))
        {
            Some(name) => Ok(Runtime::Wsl(name.clone()).to_string()),
            None => Err(WorkspaceError::NoSuchDistro {
                distro,
                installed: installed.to_vec(),
            }),
        },
    }
}

/// Whether a pane in this folder stays on Windows when Dex's terminal is WSL:
/// a worktree Windows git made records where its repository is as a `C:/...`
/// path, which Linux git cannot follow, so git there would say "not a git
/// repository". Pure: `git_file` is the folder's `.git`, when that is a file.
pub(super) fn windows_only(git_file: Option<&str>) -> bool {
    git_file
        .and_then(|text| text.trim().strip_prefix("gitdir:"))
        .map(str::trim)
        .is_some_and(|dir| {
            let bytes = dir.as_bytes();
            bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
        })
}

fn git_file(cwd: &str) -> Option<String> {
    let path = Path::new(cwd).join(".git");
    path.is_file()
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
}

/// Moves terminal panes to `runtime`: every one, or with `idle_only` those
/// whose shell is not running. Returns how many moved.
pub(super) async fn move_panes(
    state: &AppState,
    runtime: String,
    idle_only: bool,
) -> Result<usize, WorkspaceError> {
    let panes = state.db.call(|conn| store::terminal_panes(conn)).await?;
    let to_wsl = matches!(Runtime::parse(&runtime), Ok(Runtime::Wsl(_)));
    let ids: Vec<String> = panes
        .into_iter()
        .filter(|(id, _, _)| !idle_only || state.pty.shell_pid(id).is_none())
        .filter(|(_, cwd, _)| !(to_wsl && windows_only(git_file(cwd).as_deref())))
        .map(|(id, _, _)| id)
        .collect();
    let moved = ids.len();
    state
        .db
        .call(move |conn| store::update_pane_runtimes(conn, &ids, &runtime))
        .await?;
    Ok(moved)
}

/// How many processes a WSL pane's shell accounts for there on its own: the
/// login shell. The session processes `wsl.exe` starts it under belong to
/// root, so the scan cannot read them and does not count them (measured).
const WSL_SHELL_ALONE: usize = 1;

/// Switches running panes that are plain shells - nothing running in them -
/// to `runtime` now: their window sees the new runtime and starts the shell
/// again there (`syncRuntime` in the frontend). A pane running anything, an
/// agent above all, keeps its shell. Returns how many switched.
async fn switch_plain_panes(state: &AppState, runtime: &str) -> Result<usize, WorkspaceError> {
    let panes = state.db.call(|conn| store::terminal_panes(conn)).await?;
    let to_wsl = matches!(Runtime::parse(runtime), Ok(Runtime::Wsl(_)));
    let running: Vec<(String, u32, String)> = panes
        .into_iter()
        .filter(|(_, _, current)| current != runtime)
        .filter(|(_, cwd, _)| !(to_wsl && windows_only(git_file(cwd).as_deref())))
        .filter_map(|(id, _, current)| state.pty.shell_pid(&id).map(|pid| (id, pid, current)))
        .collect();
    if running.is_empty() {
        return Ok(0);
    }
    let plain = tokio::task::spawn_blocking(move || plain_shells(&running))
        .await
        .unwrap_or_default();
    let switched = plain.len();
    let target = runtime.to_owned();
    state
        .db
        .call(move |conn| store::update_pane_runtimes(conn, &plain, &target))
        .await?;
    Ok(switched)
}

/// Which of `running` - pane, shell pid, runtime - are plain shells: Windows'
/// process table read once, each distro asked once. Unknown counts as busy.
fn plain_shells(running: &[(String, u32, String)]) -> Vec<String> {
    let on_windows = running
        .iter()
        .any(|(_, _, runtime)| !matches!(Runtime::parse(runtime), Ok(Runtime::Wsl(_))));
    let procs = on_windows.then(proctree::snapshot).and_then(Result::ok);
    let mut distros: HashMap<String, Option<HashMap<String, usize>>> = HashMap::new();
    running
        .iter()
        .filter(|(pane, shell, runtime)| match Runtime::parse(runtime) {
            Ok(Runtime::Wsl(distro)) => distros
                .entry(distro.clone())
                .or_insert_with(|| wsl::pane_processes(&distro).ok())
                .as_ref()
                .and_then(|counts| counts.get(pane))
                .is_some_and(|count| *count <= WSL_SHELL_ALONE),
            _ => {
                procs
                    .as_ref()
                    .and_then(|procs| proctree::bare(procs, *shell))
                    == Some(true)
            }
        })
        .map(|(pane, _, _)| pane.clone())
        .collect()
}

/// The terminal new panes open in: what was chosen, else Windows.
pub async fn terminal(state: &AppState) -> Result<String, WorkspaceError> {
    let chosen = state.db.call(|conn| store::find_terminal(conn)).await?;
    Ok(chosen.unwrap_or_else(|| Runtime::Windows.to_string()))
}

/// `pane.terminal`: the terminal, and the choices; with `terminal`, chooses it.
/// Every pane moves to it - one not started yet when it starts, a plain shell
/// at once - except one with something running in it, which keeps its shell.
pub async fn choose_terminal(
    state: &AppState,
    args: TerminalArgs,
) -> Result<TerminalView, WorkspaceError> {
    let distros = tokio::task::spawn_blocking(wsl::distros)
        .await
        .unwrap_or_default();
    if let Some(requested) = args.terminal {
        let checked = check(&requested, &distros)?;
        let stored = checked.clone();
        state
            .db
            .call(move |conn| store::update_terminal(conn, &stored))
            .await?;
        let moved = move_panes(state, checked.clone(), true).await?;
        if moved + switch_plain_panes(state, &checked).await? > 0 {
            state.bus.publish("workspaces");
        }
    }
    Ok(TerminalView {
        terminal: terminal(state).await?,
        runtimes: std::iter::once(Runtime::Windows)
            .chain(distros.into_iter().map(Runtime::Wsl))
            .map(|runtime| runtime.to_string())
            .collect(),
    })
}

/// As Dex starts, before any shell has: every terminal pane opens in the
/// chosen terminal, which is what choosing it means. A chosen distro that has
/// since been uninstalled would leave every pane dead, so Dex goes back to
/// Windows, and says so.
pub async fn open_panes_in_terminal(state: &AppState) -> Result<(), WorkspaceError> {
    let mut runtime = terminal(state).await?;
    if let Ok(Runtime::Wsl(distro)) = Runtime::parse(&runtime) {
        let installed = tokio::task::spawn_blocking(wsl::distros)
            .await
            .unwrap_or_default();
        if !installed
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&distro))
        {
            tracing::warn!(%distro, "the chosen WSL distro is not installed; Dex opens in Windows");
            runtime = Runtime::Windows.to_string();
            let windows = runtime.clone();
            state
                .db
                .call(move |conn| store::update_terminal(conn, &windows))
                .await?;
        }
    }
    move_panes(state, runtime, false).await.map(|_| ())
}
