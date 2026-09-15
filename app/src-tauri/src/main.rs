//! The Dex desktop app: hosts the UI and embeds the daemon (`dex-core`) on a
//! background tokio runtime. There is no standalone daemon (docs/prd.md §4).
#![forbid(unsafe_code)]
// Release builds are GUI-only; without this Windows also opens a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daemon;
mod pty;

use dex_core::app::AppState;
use dex_core::platform::pty::{FlowLimits, PtySupervisor};
use dex_core::platform::{job, paths};

fn main() {
    // Logs go to stderr for now; the file sink under %LOCALAPPDATA%\Dex\logs
    // arrives with the daemon (M4).
    let _ = tracing_subscriber::fmt().try_init();

    // Before any child is spawned: every descendant inherits the job, so if
    // this process dies for any reason, Windows kills the whole tree. The
    // binding must live until the process ends (dropping it kills us).
    let _job = match job::contain_current_process() {
        Ok(job) => Some(job),
        Err(err) => {
            tracing::error!(%err, "no kill-on-close job; child processes may outlive a crash");
            None
        }
    };

    let state = match open_state() {
        Ok(state) => state,
        Err(err) => {
            tracing::error!(%err, "cannot open the Dex database");
            eprintln!("dex: cannot open the database: {err}");
            std::process::exit(1);
        }
    };

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .manage(PtySupervisor::new(FlowLimits::default()))
        .invoke_handler(tauri::generate_handler![
            daemon::dex_request,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_ack,
            pty::pty_kill,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("dex failed to start: {err}");
        std::process::exit(1);
    }
}

/// Opens (creating and migrating if needed) `%APPDATA%\Dex\dex.db`.
fn open_state() -> Result<AppState, String> {
    let dir = paths::app_data_dir().map_err(|err| format!("app data directory: {err}"))?;
    AppState::open(&dir.join("dex.db")).map_err(|err| err.to_string())
}
