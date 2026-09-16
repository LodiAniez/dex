//! Windows toast notifications for agents that need attention (PRD §9.5).
//!
//! The UI decides when to notify, because it knows which pane has focus; this
//! shows the toast, and when it is clicked brings Dex forward and tells the UI
//! which pane to show. Windows only shows toasts for registered apps. The
//! installer registers Dex's id; a build that was never installed (a dev
//! build, `--no-bundle`) has no registration, and borrows PowerShell's id so
//! toasts still appear rather than vanishing.

use std::process::Command;
use std::sync::OnceLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_winrt_notification::Toast;

/// Emitted when a toast is clicked; the UI switches to the pane.
const FOCUS_EVENT: &str = "dex://focus-pane";

/// Dex's AppUserModelID, as the MSI registers it (`wix/dex.wxs`) and as
/// Tauri stamps it onto the Start Menu shortcut: the bundle identifier.
const APP_ID: &str = "dev.dex.desktop";

/// Hides the console `reg.exe` would otherwise flash open.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The id toasts are shown under. Checked once: whether Dex's own id is
/// registered does not change while the app runs.
fn app_id() -> &'static str {
    static ID: OnceLock<&'static str> = OnceLock::new();
    ID.get_or_init(|| {
        use std::os::windows::process::CommandExt;
        let registered = Command::new("reg")
            .args([
                "query",
                &format!(r"HKCU\Software\Classes\AppUserModelId\{APP_ID}"),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        if registered {
            APP_ID
        } else {
            tracing::info!(
                "Dex's app id is not registered (not installed?); toasts use PowerShell's"
            );
            Toast::POWERSHELL_APP_ID
        }
    })
}

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
    Toast::new(app_id())
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
