//! The Dex desktop app: hosts the UI and embeds the daemon (`dex-core`) on a
//! background tokio runtime. There is no standalone daemon (docs/prd.md §4).
#![forbid(unsafe_code)]
// Release builds are GUI-only; without this Windows also opens a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daemon;
mod notify;
mod pty;
mod setup;

use std::path::PathBuf;

use dex_core::app::AppState;
use dex_core::platform::auth::Token;
use dex_core::platform::{job, paths, pipe};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

fn main() {
    // Logs go to stderr for now; a file sink under %LOCALAPPDATA%\Dex\logs is still to come.
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

    let (state, token, token_path) = match startup() {
        Ok(started) => started,
        Err(err) => {
            tracing::error!(%err, "Dex cannot start");
            eprintln!("dex: {err}");
            std::process::exit(1);
        }
    };
    // Held for the life of the process, like the job: dropping the watcher
    // stops config edits being noticed, silently.
    let _config_watcher = match state.config.watch(state.bus.clone()) {
        Ok(watcher) => Some(watcher),
        Err(err) => {
            tracing::error!(%err, "config changes will need a restart to take effect");
            None
        }
    };
    let pipe_name = pipe::default_name();
    let daemon_state = state.clone();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            // Bound inside the async runtime: tokio's pipes register with its reactor.
            match tauri::async_runtime::block_on(async { pipe::bind(&pipe_name) }) {
                Ok(server) => {
                    // Only now that the pipe is ours: a second copy of Dex that
                    // fails to bind must not replace the running app's token.
                    if let Err(err) = token.write(&token_path) {
                        tracing::error!(%err, "cannot write the token file; `dex` commands will be refused");
                    }
                    tauri::async_runtime::spawn(daemon::serve_pipe(
                        server,
                        pipe_name,
                        token,
                        daemon_state.clone(),
                    ));
                    tauri::async_runtime::spawn(daemon::forward_changes(
                        app.handle().clone(),
                        daemon_state.clone(),
                    ));
                    tauri::async_runtime::spawn(daemon::watch_agents(daemon_state));
                }
                Err(err) => {
                    // Someone else holds the name, most likely another Dex.
                    // Serving beside it would let a stranger answer our
                    // clients, so this copy refuses to run (PRD §6.1).
                    tracing::error!(%err, "cannot bind the Dex pipe");
                    app.dialog()
                        .message(format!(
                            "Dex could not open its control pipe:\n{err}\n\nAnother copy of Dex is probably running. Close it, then start Dex again."
                        ))
                        .title("Dex is already running")
                        .kind(MessageDialogKind::Error)
                        .show(|_| std::process::exit(1));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            daemon::dex_request,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_ack,
            pty::pty_kill,
            notify::notify_agent,
            setup::setup_check,
            setup::setup_run,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("dex failed to start: {err}");
        std::process::exit(1);
    }
}

/// Opens `%APPDATA%\Dex\dex.db`, reads `config.toml`, and prepares a fresh
/// connection token (written once the pipe is bound).
fn startup() -> Result<(AppState, Token, PathBuf), String> {
    let dir = paths::app_data_dir().map_err(|err| format!("app data directory: {err}"))?;
    let (state, problems) = AppState::open(&dir.join("dex.db"), dir.join("config.toml"))
        .map_err(|err| format!("database: {err}"))?;
    // Logged, not fatal: a setting Dex could not use is a reason to say so, not
    // a reason to refuse to start.
    for problem in problems {
        tracing::warn!("{problem}");
    }
    let token = Token::generate().map_err(|err| format!("token: {err}"))?;
    Ok((state, token, dir.join("token")))
}
