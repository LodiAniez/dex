//! Tauri commands bridging the frontend's terminals to the daemon's PTY supervisor.
//!
//! Output travels over per-pane Tauri Channels as raw bytes: events evaluate
//! JavaScript per message and would choke on terminal throughput (PRD §7.1).

use std::path::PathBuf;

use dex_core::app::AppState;
use dex_core::platform::pipe;
use dex_core::platform::pty::{OutputSink, PtyOutput, SpawnRequest, resolve_shell, shell_args};
use dex_core::platform::wsl::{self, Runtime};
use dex_core::platform::{login_env, paths};
use dex_core::router;
use dex_protocol::Request;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody};

/// Non-output news about a pane, sent on its event channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PtyEvent {
    /// Output was discarded while the display was unresponsive.
    Dropped { bytes: u64 },
    /// The process exited.
    Exited { code: Option<u32> },
}

/// Which pane to start, and at what size.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnPane {
    pane_id: String,
    workspace_id: Option<String>,
    cwd: Option<String>,
    /// `windows` (the default) or `wsl:<distro>`.
    runtime: Option<String>,
    cols: u16,
    rows: u16,
}

/// Starts the default shell in a new PTY for a pane. The shell learns where it
/// is through `DEX_PANE_ID` / `DEX_WORKSPACE_ID`, and how to reach the daemon
/// through `DEX_SOCKET`, so `dex` commands run inside it just work.
#[tauri::command]
pub async fn pty_spawn(
    state: State<'_, AppState>,
    pane: SpawnPane,
    on_output: Channel<InvokeResponseBody>,
    on_event: Channel<PtyEvent>,
) -> Result<(), String> {
    let runtime = match pane.runtime.as_deref() {
        Some(text) => Runtime::parse(text)?,
        None => Runtime::Windows,
    };
    let cwd = pane
        .cwd
        .map(PathBuf::from)
        .or_else(paths::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    let (program, args) = match &runtime {
        Runtime::Windows => {
            let shell =
                resolve_shell(state.config.get().shell.as_deref()).ok_or(if cfg!(windows) {
                    "no shell found: pwsh.exe, powershell.exe and cmd.exe are all missing from PATH"
                } else {
                    "no shell found: $SHELL, /bin/zsh, /bin/bash and /bin/sh are all missing"
                })?;
            (shell, shell_args(cfg!(unix)))
        }
        // The distro's own login shell, started in the pane's folder, which
        // `wsl.exe` translates (`C:/src` is `/mnt/c/src` inside). Checked here:
        // `wsl.exe` only says ERROR_FILE_NOT_FOUND about a folder that has
        // gone, such as a worktree since removed.
        Runtime::Wsl(distro) => {
            if !cwd.is_dir() {
                return Err(format!(
                    "the pane's folder {} no longer exists",
                    cwd.display()
                ));
            }
            (wsl_exe(), wsl::pane_args(distro, &paths::normalize(&cwd)))
        }
    };
    let socket = pipe::socket_env(&pipe::default_name()).map_err(|err| err.to_string())?;
    let mut env = vec![
        ("DEX_PANE_ID".to_owned(), pane.pane_id.clone()),
        ("DEX_SOCKET".to_owned(), socket),
    ];
    // An app started from the Finder has no terminal type and no locale.
    // Nor is `dex`, inside the app bundle, on anyone's PATH.
    if cfg!(unix) {
        env.extend(login_env::pane_defaults(|name| std::env::var_os(name)));
        let dex_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from));
        if let Some(dex_dir) = dex_dir {
            let inherited = std::env::var("PATH").ok();
            let base = login_env::login_path().or(inherited.as_deref());
            env.push(("PATH".to_owned(), login_env::pane_path(&dex_dir, base)));
        }
    }
    if let Some(workspace_id) = pane.workspace_id {
        env.push(("DEX_WORKSPACE_ID".to_owned(), workspace_id));
    }
    // Into Linux, and back out to `dex.exe` when a hook there runs it.
    if let Runtime::Wsl(_) = runtime {
        let owners = std::env::var("WSLENV").ok();
        env.push((
            "WSLENV".to_owned(),
            wsl::wslenv(owners.as_deref(), &wsl::PASSED_IN),
        ));
    }

    let request = SpawnRequest {
        pane_id: pane.pane_id,
        args,
        program,
        cwd,
        env,
        cols: pane.cols,
        rows: pane.rows,
    };
    let sink = window_sink(state.inner(), &request.pane_id, on_output, on_event);
    state
        .pty
        .spawn(request, sink)
        .map_err(|err| err.to_string())
}

/// `wsl.exe`, from System32 rather than whatever `PATH` finds first.
fn wsl_exe() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32").join("wsl.exe"))
        .unwrap_or_else(|| PathBuf::from("wsl.exe"))
}

/// A pane's output, sent to one window's channels. The process's exit also
/// ends whatever agent ran in the pane (PRD §9.2), whichever window shows it.
fn window_sink(
    state: &AppState,
    pane_id: &str,
    on_output: Channel<InvokeResponseBody>,
    on_event: Channel<PtyEvent>,
) -> OutputSink {
    let exited_pane = pane_id.to_owned();
    let daemon = state.clone();
    Box::new(move |output: PtyOutput| {
        if let PtyOutput::Exited { .. } = &output {
            let (daemon, pane) = (daemon.clone(), exited_pane.clone());
            tauri::async_runtime::spawn(async move {
                let exit = Request {
                    id: "pty-exit".into(),
                    cmd: "agent.pane_exited".into(),
                    args: json!({ "pane": pane }),
                };
                router::dispatch(&daemon, exit).await;
            });
        }
        // A send only fails once the window is gone; nobody is left to tell.
        let _ = match output {
            PtyOutput::Data(bytes) => on_output.send(InvokeResponseBody::Raw(bytes)),
            PtyOutput::Dropped { bytes } => on_event.send(PtyEvent::Dropped { bytes }),
            PtyOutput::Exited { code } => on_event.send(PtyEvent::Exited { code }),
        };
    })
}

/// First half of moving a pane between windows: its output is kept from now
/// on. Returns the bytes its current window has been sent, for that window to
/// wait for before it serializes its screen (`relay.rs` in dex-core).
#[tauri::command]
pub async fn pty_hold(state: State<'_, AppState>, pane_id: String) -> Result<u64, String> {
    state.pty.hold(&pane_id).map_err(|err| err.to_string())
}

/// Second half: the calling window takes the pane's output - what was kept,
/// then everything after.
#[tauri::command]
pub async fn pty_attach(
    state: State<'_, AppState>,
    pane_id: String,
    on_output: Channel<InvokeResponseBody>,
    on_event: Channel<PtyEvent>,
) -> Result<(), String> {
    let sink = window_sink(state.inner(), &pane_id, on_output, on_event);
    state
        .pty
        .attach(&pane_id, sink)
        .map_err(|err| err.to_string())
}

/// Sends keyboard input to a pane.
#[tauri::command]
pub async fn pty_write(
    state: State<'_, AppState>,
    pane_id: String,
    data: String,
) -> Result<(), String> {
    state
        .pty
        .write(&pane_id, data.as_bytes())
        .map_err(|err| err.to_string())
}

/// Resizes a pane (already debounced by the frontend).
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    pane_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state
        .pty
        .resize(&pane_id, cols, rows)
        .map_err(|err| err.to_string())
}

/// Acknowledges output the terminal has finished rendering (flow control).
#[tauri::command]
pub async fn pty_ack(
    state: State<'_, AppState>,
    pane_id: String,
    bytes: usize,
) -> Result<(), String> {
    state
        .pty
        .ack(&pane_id, bytes)
        .map_err(|err| err.to_string())
}

/// Kills a pane's process.
#[tauri::command]
pub async fn pty_kill(state: State<'_, AppState>, pane_id: String) -> Result<(), String> {
    state.pty.kill(&pane_id).map_err(|err| err.to_string())
}
