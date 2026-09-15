//! Tauri commands bridging the frontend's terminals to `dex-core`'s PTY supervisor.
//!
//! Output travels over per-pane Tauri Channels as raw bytes: events evaluate
//! JavaScript per message and would choke on terminal throughput (PRD §7.1).

use std::path::PathBuf;

use dex_core::platform::pty::{PtyOutput, PtySupervisor, SpawnRequest, resolve_shell};
use serde::Serialize;
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

/// Starts the default shell in a new PTY for `pane_id`.
#[tauri::command]
pub async fn pty_spawn(
    supervisor: State<'_, PtySupervisor>,
    pane_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
    on_event: Channel<PtyEvent>,
) -> Result<(), String> {
    let program = resolve_shell(None)
        .ok_or("no shell found: pwsh.exe, powershell.exe and cmd.exe are all missing from PATH")?;
    let cwd = cwd
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);

    let request = SpawnRequest {
        env: vec![("DEX_PANE_ID".into(), pane_id.clone())],
        pane_id,
        program,
        args: Vec::new(),
        cwd,
        cols,
        rows,
    };
    let sink = Box::new(move |output: PtyOutput| {
        // A send only fails once the window is gone; nobody is left to tell.
        let _ = match output {
            PtyOutput::Data(bytes) => on_output.send(InvokeResponseBody::Raw(bytes)),
            PtyOutput::Dropped { bytes } => on_event.send(PtyEvent::Dropped { bytes }),
            PtyOutput::Exited { code } => on_event.send(PtyEvent::Exited { code }),
        };
    });
    supervisor
        .spawn(request, sink)
        .map_err(|err| err.to_string())
}

/// Sends keyboard input to a pane.
#[tauri::command]
pub async fn pty_write(
    supervisor: State<'_, PtySupervisor>,
    pane_id: String,
    data: String,
) -> Result<(), String> {
    supervisor
        .write(&pane_id, data.as_bytes())
        .map_err(|err| err.to_string())
}

/// Resizes a pane (already debounced by the frontend).
#[tauri::command]
pub async fn pty_resize(
    supervisor: State<'_, PtySupervisor>,
    pane_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    supervisor
        .resize(&pane_id, cols, rows)
        .map_err(|err| err.to_string())
}

/// Acknowledges output the terminal has finished rendering (flow control).
#[tauri::command]
pub async fn pty_ack(
    supervisor: State<'_, PtySupervisor>,
    pane_id: String,
    bytes: usize,
) -> Result<(), String> {
    supervisor
        .ack(&pane_id, bytes)
        .map_err(|err| err.to_string())
}

/// Kills a pane's process.
#[tauri::command]
pub async fn pty_kill(supervisor: State<'_, PtySupervisor>, pane_id: String) -> Result<(), String> {
    supervisor.kill(&pane_id).map_err(|err| err.to_string())
}
