//! One handler per IPC command owned by the diagnostics slice. Orchestrates store, logic, and platform.

use dex_protocol::config::ConfigView;

use super::model::DiagnosticsError;
use crate::app::AppState;

/// `config.get`: the settings in force, without re-reading the file.
///
/// Reports what was wrong the last time it *was* read, which is usually
/// startup. A problem found then would otherwise only reach the log, and the
/// app is a GUI — this is where the owner will ask.
pub fn get_config(state: &AppState) -> Result<ConfigView, DiagnosticsError> {
    view(state, state.config.problems())
}

/// `config.reload`: re-reads the file and reports what was wrong with it.
///
/// The watcher normally does this, but it cannot see every filesystem — a
/// config on a network share or a synced folder may never raise an event — so
/// there has to be a way to ask.
pub fn reload_config(state: &AppState) -> Result<ConfigView, DiagnosticsError> {
    let problems = state.config.reload();
    view(state, problems)
}

fn view(state: &AppState, problems: Vec<String>) -> Result<ConfigView, DiagnosticsError> {
    let path = state.config.path().to_path_buf();
    let settings = state.config.get();
    let effective = toml::to_string_pretty(settings.as_ref())
        .map_err(|err| DiagnosticsError::Unprintable(err.to_string()))?;
    Ok(ConfigView {
        present: path.is_file(),
        path: crate::platform::paths::normalize(&path),
        keys: settings.keys.clone(),
        max_concurrent: settings.agents.max_concurrent,
        office_view: settings.ui.office_view.clone(),
        problems,
        effective,
    })
}
