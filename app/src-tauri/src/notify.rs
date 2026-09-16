//! Windows toast notifications for agents that need attention (PRD §9.5).
//!
//! The UI decides when to notify, because it knows which pane has focus; this
//! shows the toast, and when it is clicked brings Dex forward and tells the UI
//! which pane to show. Windows only shows toasts for registered apps, so
//! until the installer registers Dex's own app id (M8) toasts borrow
//! PowerShell's and say so.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_winrt_notification::Toast;

/// Emitted when a toast is clicked; the UI switches to the pane.
const FOCUS_EVENT: &str = "dex://focus-pane";

#[derive(Debug, Clone, Serialize)]
struct FocusPane {
    workspace: String,
    pane: String,
}

/// Shows a toast about the agent in `pane`; clicking it focuses that pane.
#[tauri::command]
pub fn notify_agent(
    app: AppHandle,
    title: String,
    body: String,
    workspace: String,
    pane: String,
) -> Result<(), String> {
    let target = FocusPane { workspace, pane };
    Toast::new(Toast::POWERSHELL_APP_ID)
        .title(&title)
        .text1(&body)
        .on_activated(move |_action| {
            if let Some(window) = app.get_webview_window("main") {
                // Best effort: the window may already be in front.
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            let _ = app.emit(FOCUS_EVENT, target.clone());
            Ok(())
        })
        .show()
        .map_err(|err| err.to_string())
}
