//! Moving panes that are already running to a newly chosen terminal - only
//! when the owner says so.
//!
//! Whether a shell is "plain", with nothing running in it, cannot be known for
//! sure: PowerShell runs most commands inside its own process, and in WSL a
//! `sudo` hides what it runs. So Dex never restarts a running pane on its own.
//! After a choice it lists the panes running elsewhere, each marked busy when
//! something may be running in it, and the window restarts the ones the owner
//! confirms. A pane's `runtime` always says where its shell actually runs: it
//! changes when a shell starts (`pane.started`), never ahead of it, so the
//! watchdog looks for an agent where it really is.

use std::collections::HashMap;

use dex_protocol::pane::{PaneStartedArgs, RunningPane};

use super::model::WorkspaceError;
use super::runtimes::{git_file, windows_only};
use super::store;
use crate::app::AppState;
use crate::platform::proctree::{self, Proc};
use crate::platform::wsl::{self, Runtime};

/// How many processes a WSL pane's shell accounts for there on its own: the
/// login shell. The session processes `wsl.exe` starts it under belong to
/// root, so the scan cannot read them and does not count them (measured).
const WSL_SHELL_ALONE: usize = 1;

/// Whether something may be running in a pane: anything Dex cannot rule out.
/// `procs` is Windows' process table and `counts` the pane's distro's
/// processes per pane, each `None` when it could not be read. Pure.
pub(super) fn busy(
    pane: &str,
    shell: u32,
    runtime: &str,
    procs: Option<&[Proc]>,
    counts: Option<&HashMap<String, usize>>,
) -> bool {
    // Restarting a pane is the destructive one, so what cannot be told is left
    // alone: busy.
    running_under(pane, shell, runtime, procs, counts).unwrap_or(true)
}

/// Whether anything is running under a pane's shell, or `None` when that
/// cannot be told: the shell is not in the process table, or the distro did
/// not answer.
///
/// The judgement both callers share. They differ in what to do with `None`,
/// which is why that is left to them.
pub(super) fn running_under(
    pane: &str,
    shell: u32,
    runtime: &str,
    procs: Option<&[Proc]>,
    counts: Option<&HashMap<String, usize>>,
) -> Option<bool> {
    match Runtime::parse(runtime) {
        Ok(Runtime::Wsl(_)) => counts
            .and_then(|counts| counts.get(pane))
            .map(|count| *count > WSL_SHELL_ALONE),
        _ => proctree::bare(procs?, shell).map(|bare| !bare),
    }
}

/// For the agent slice: whether anything is running under each of these panes'
/// shells (issue #81).
///
/// A pane with no shell at all answers `Some(false)`: nothing can be running
/// under a shell that is not there, and that is a different thing from not
/// being able to tell. The process table is read once and each distro asked
/// once, however many panes are named.
pub async fn anything_running(state: &AppState, panes: &[String]) -> HashMap<String, Option<bool>> {
    let wanted: Vec<String> = panes.to_vec();
    let rows = state
        .db
        .call(|conn| store::terminal_panes(conn))
        .await
        .unwrap_or_default();
    let asked: Vec<(String, String, Option<u32>)> = rows
        .into_iter()
        .filter(|(pane, _, _)| wanted.iter().any(|want| want == pane))
        .map(|(pane, _, runtime)| {
            let shell = state.pty.shell_pid(&pane);
            (pane, runtime, shell)
        })
        .collect();
    let on_windows = asked
        .iter()
        .any(|(_, runtime, _)| !matches!(Runtime::parse(runtime), Ok(Runtime::Wsl(_))));
    let procs = match on_windows {
        true => tokio::task::spawn_blocking(proctree::snapshot)
            .await
            .ok()
            .and_then(Result::ok),
        false => None,
    };
    let mut distros: HashMap<String, Option<HashMap<String, usize>>> = HashMap::new();
    let mut running = HashMap::new();
    for (pane, runtime, shell) in asked {
        let Some(shell) = shell else {
            // No shell, so nothing under one.
            running.insert(pane, Some(false));
            continue;
        };
        let counts = match Runtime::parse(&runtime) {
            Ok(Runtime::Wsl(distro)) => {
                if !distros.contains_key(&distro) {
                    let asked_for = distro.clone();
                    let answer =
                        tokio::task::spawn_blocking(move || wsl::pane_processes(&asked_for).ok())
                            .await
                            .ok()
                            .flatten();
                    distros.insert(distro.clone(), answer);
                }
                distros.get(&distro).and_then(Option::as_ref)
            }
            _ => None,
        };
        running.insert(
            pane.clone(),
            running_under(&pane, shell, &runtime, procs.as_deref(), counts),
        );
    }
    running
}

/// The panes whose shell is running in another terminal than `chosen`, less
/// those that must stay where they are (a worktree Windows git made, when the
/// choice is WSL).
pub(super) async fn running_elsewhere(
    state: &AppState,
    chosen: &str,
) -> Result<Vec<RunningPane>, WorkspaceError> {
    let rows = state
        .db
        .call(|conn| store::terminal_panes_named(conn))
        .await?;
    let to_wsl = matches!(Runtime::parse(chosen), Ok(Runtime::Wsl(_)));
    let running: Vec<(store::TerminalPaneRow, u32)> = rows
        .into_iter()
        .filter(|row| row.runtime != chosen)
        .filter(|row| !(to_wsl && windows_only(git_file(&row.cwd).as_deref())))
        .filter_map(|row| state.pty.shell_pid(&row.id).map(|shell| (row, shell)))
        .collect();
    if running.is_empty() {
        return Ok(Vec::new());
    }
    Ok(tokio::task::spawn_blocking(move || judge(running))
        .await
        .unwrap_or_default())
}

/// Marks each running pane busy or not: Windows' table read once, each
/// distro asked once. Blocking.
fn judge(running: Vec<(store::TerminalPaneRow, u32)>) -> Vec<RunningPane> {
    let on_windows = running
        .iter()
        .any(|(row, _)| !matches!(Runtime::parse(&row.runtime), Ok(Runtime::Wsl(_))));
    let procs = on_windows.then(proctree::snapshot).and_then(Result::ok);
    let mut distros: HashMap<String, Option<HashMap<String, usize>>> = HashMap::new();
    running
        .into_iter()
        .map(|(row, shell)| {
            let counts = match Runtime::parse(&row.runtime) {
                Ok(Runtime::Wsl(distro)) => distros
                    .entry(distro.clone())
                    .or_insert_with(|| wsl::pane_processes(&distro).ok())
                    .as_ref(),
                _ => None,
            };
            RunningPane {
                busy: busy(&row.id, shell, &row.runtime, procs.as_deref(), counts),
                pane: row.id,
                workspace: row.workspace,
                label: row.label,
                cwd: row.cwd,
                runtime: row.runtime,
            }
        })
        .collect()
}

/// `pane.started`: a pane's shell has started, in `runtime` - what its window
/// asked for, and where it now runs. Announced when that is new.
pub async fn record_started(state: &AppState, args: PaneStartedArgs) -> Result<(), WorkspaceError> {
    let runtime = Runtime::parse(&args.runtime).map_err(WorkspaceError::InvalidRuntime)?;
    let runtime = runtime.to_string();
    let changed = state
        .db
        .call(move |conn| store::update_pane_runtime(conn, &args.pane, &runtime))
        .await?;
    if changed {
        state.bus.publish("workspaces");
    }
    Ok(())
}
