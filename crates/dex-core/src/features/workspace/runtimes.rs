//! Which terminal Dex opens: PowerShell (or the configured shell) on Windows,
//! or a WSL distro's shell (`platform/wsl.rs`). One choice for the whole app,
//! kept in `app_state`: every new pane opens there - a new workspace's, a
//! split, a spawned agent's - and when Dex starts, every pane does. A pane
//! records the terminal its shell was started in as its `runtime`, so one
//! already running keeps going where it is when the choice changes.

use dex_protocol::pane::{TerminalArgs, TerminalView};

use super::model::WorkspaceError;
use super::store;
use crate::app::AppState;
use crate::platform::wsl::{self, Runtime};

/// Checks a terminal against the distros installed. Pure: `installed` is what
/// `wsl.exe` listed.
pub(super) fn check(text: &str, installed: &[String]) -> Result<String, WorkspaceError> {
    match Runtime::parse(text).map_err(WorkspaceError::InvalidRuntime)? {
        Runtime::Windows => Ok(Runtime::Windows.to_string()),
        Runtime::Wsl(distro) if installed.contains(&distro) => Ok(Runtime::Wsl(distro).to_string()),
        Runtime::Wsl(distro) => Err(WorkspaceError::NoSuchDistro {
            distro,
            installed: installed.to_vec(),
        }),
    }
}

/// The terminal new panes open in: what was chosen, else Windows.
pub async fn terminal(state: &AppState) -> Result<String, WorkspaceError> {
    let chosen = state.db.call(|conn| store::find_terminal(conn)).await?;
    Ok(chosen.unwrap_or_else(|| Runtime::Windows.to_string()))
}

/// `pane.terminal`: the terminal, and the choices; with `terminal`, chooses it.
/// Panes already running keep their shells; the next ones open in it.
pub async fn choose_terminal(
    state: &AppState,
    args: TerminalArgs,
) -> Result<TerminalView, WorkspaceError> {
    let distros = tokio::task::spawn_blocking(wsl::distros)
        .await
        .unwrap_or_default();
    if let Some(requested) = args.terminal {
        let checked = check(&requested, &distros)?;
        state
            .db
            .call(move |conn| store::update_terminal(conn, &checked))
            .await?;
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
/// chosen terminal, which is what choosing it means.
pub async fn open_panes_in_terminal(state: &AppState) -> Result<(), WorkspaceError> {
    let runtime = terminal(state).await?;
    state
        .db
        .call(move |conn| store::update_terminal_panes(conn, &runtime))
        .await?;
    Ok(())
}
