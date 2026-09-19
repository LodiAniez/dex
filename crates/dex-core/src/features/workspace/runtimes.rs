//! Where a pane's shell runs: on Windows, or inside a WSL distro
//! (`platform/wsl.rs`). A pane records it as its `runtime`; a split takes the
//! split pane's unless it is told otherwise.

use dex_protocol::pane::RuntimeList;

use super::model::WorkspaceError;
use crate::platform::wsl::{self, Runtime};

/// Checks a requested runtime against the distros installed. Pure: `installed`
/// is what `wsl.exe` listed.
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

/// A requested runtime, checked; `None` when none was asked for.
pub(super) async fn runtime_arg(requested: Option<&str>) -> Result<Option<String>, WorkspaceError> {
    let Some(text) = requested else {
        return Ok(None);
    };
    // Only a WSL runtime needs the list, which costs a `wsl.exe` run.
    let installed = match Runtime::parse(text) {
        Ok(Runtime::Wsl(_)) => tokio::task::spawn_blocking(wsl::distros)
            .await
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    check(text, &installed).map(Some)
}

/// For other slices: a requested runtime, checked against what is installed.
pub async fn checked_runtime(requested: &str) -> Result<String, WorkspaceError> {
    runtime_arg(Some(requested))
        .await
        .map(|checked| checked.unwrap_or_else(|| requested.to_owned()))
}

/// `pane.runtimes`: Windows, then each installed distro.
pub async fn list_runtimes() -> RuntimeList {
    let distros = tokio::task::spawn_blocking(wsl::distros)
        .await
        .unwrap_or_default();
    RuntimeList {
        runtimes: std::iter::once(Runtime::Windows)
            .chain(distros.into_iter().map(Runtime::Wsl))
            .map(|runtime| runtime.to_string())
            .collect(),
    }
}
