//! Tauri commands bridging the frontend's terminals to the daemon's PTY supervisor.
//!
//! Output travels over per-pane Tauri Channels as raw bytes: events evaluate
//! JavaScript per message and would choke on terminal throughput (PRD §7.1).

use std::path::PathBuf;

use dex_core::app::AppState;
use dex_core::platform::pipe;
use dex_core::platform::pty::{PtyOutput, SpawnRequest, resolve_shell};
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
    let program = resolve_shell(state.config.get().shell.as_deref())
        .ok_or("no shell found: pwsh.exe, powershell.exe and cmd.exe are all missing from PATH")?;
    let cwd = pane
        .cwd
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    let mut env = vec![
        ("DEX_PANE_ID".to_owned(), pane.pane_id.clone()),
        (
            "DEX_SOCKET".to_owned(),
            format!("pipe:{}", pipe::default_name()),
        ),
    ];
    if let Some(workspace_id) = pane.workspace_id {
        env.push(("DEX_WORKSPACE_ID".to_owned(), workspace_id));
    }

    let request = SpawnRequest {
        pane_id: pane.pane_id,
        program,
        args: Vec::new(),
        cwd,
        env,
        cols: pane.cols,
        rows: pane.rows,
    };
    let exited_pane = request.pane_id.clone();
    let daemon = state.inner().clone();
    let sink = Box::new(move |output: PtyOutput| {
        if let PtyOutput::Exited { .. } = &output {
            // Whatever agent ran in this pane ended with its process (PRD §9.2).
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
    });
    state
        .pty
        .spawn(request, sink)
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
