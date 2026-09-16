//! The ways into the daemon: the UI's `dex_request` command and the named
//! pipe for the CLI and MCP server. Both send the same request envelope and
//! both go through `router::dispatch`, so every command has exactly one
//! implementation and everyone gets the same repair strings.

use dex_core::app::AppState;
use dex_core::platform::auth::Token;
use dex_core::platform::bus::RecvError;
use dex_core::platform::pipe::{self, NamedPipeServer};
use dex_core::router;
use dex_protocol::{Request, Response};
use tauri::{AppHandle, Emitter, State};

use std::time::Duration;

/// The event the UI listens to; its payload is the topic that changed.
const CHANGED_EVENT: &str = "dex://changed";

/// Runs one daemon command for the UI. Command errors come back inside the
/// `Response`; this never fails.
#[tauri::command]
pub async fn dex_request(state: State<'_, AppState>, request: Request) -> Result<Response, String> {
    Ok(router::dispatch(&state, request).await)
}

/// Serves the named pipe until the app exits.
pub async fn serve_pipe(server: NamedPipeServer, name: String, token: Token, state: AppState) {
    pipe::serve(server, name, token, move |request: Request| {
        let state = state.clone();
        async move { router::dispatch(&state, request).await }
    })
    .await;
}

/// Tells the UI whenever daemon state changes — for example from the CLI — so
/// it re-reads. Runs until the app exits.
pub async fn forward_changes(app: AppHandle, state: AppState) {
    let mut changes = state.bus.subscribe();
    loop {
        let topic = match changes.recv().await {
            Ok(changed) => changed.topic,
            // Missed some notifications: one catch-all makes the UI re-read everything.
            Err(RecvError::Lagged(_)) => "all",
            Err(RecvError::Closed) => return,
        };
        // Fails only while the window is being torn down; nothing to do then.
        let _ = app.emit(CHANGED_EVENT, topic);
    }
}

/// How often the watchdog looks for agents that have gone silent.
const WATCHDOG_EVERY: Duration = Duration::from_secs(15);

/// Runs the agent watchdog (`agent.sweep`) until the app exits (PRD §9.2).
pub async fn watch_agents(state: AppState) {
    loop {
        tokio::time::sleep(WATCHDOG_EVERY).await;
        let sweep = Request {
            id: "watchdog".into(),
            cmd: "agent.sweep".into(),
            args: serde_json::json!({}),
        };
        router::dispatch(&state, sweep).await;
    }
}
