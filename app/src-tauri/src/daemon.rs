//! The frontend's way into the daemon.
//!
//! The UI sends the same request/response envelope the CLI will send over the
//! pipe, and both go through `router::dispatch` — so every command has exactly
//! one implementation, and the UI gets the same repair strings as agents.

use dex_core::app::AppState;
use dex_core::router;
use dex_protocol::{Request, Response};
use tauri::State;

/// Runs one daemon command for the UI. Command errors come back inside the
/// `Response`; this never fails.
#[tauri::command]
pub async fn dex_request(state: State<'_, AppState>, request: Request) -> Result<Response, String> {
    Ok(router::dispatch(&state, request).await)
}
