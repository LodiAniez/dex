//! Notifications for agents that need attention (PRD §9.5): Windows toasts,
//! and on macOS a Notification Center banner.
//!
//! The UI decides when to notify, because it knows which pane has focus; this
//! shows the toast, and when it is clicked brings Dex forward and tells the UI
//! which pane to show. Windows only shows toasts for registered apps. The
//! installer registers Dex's id; a build that was never installed (a dev
//! build, `--no-bundle`) has no registration, and borrows PowerShell's id so
//! toasts still appear rather than vanishing.

use std::process::Command;
#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
use serde::Serialize;
use tauri::AppHandle;
#[cfg(windows)]
use tauri::{Emitter, Manager};
#[cfg(windows)]
use tauri_winrt_notification::Toast;

/// Emitted when a toast is clicked; the UI switches to the pane.
#[cfg(windows)]
const FOCUS_EVENT: &str = "dex://focus-pane";

/// Dex's AppUserModelID, as the MSI registers it (`wix/dex.wxs`) and as
/// Tauri stamps it onto the Start Menu shortcut: the bundle identifier.
#[cfg(windows)]
const APP_ID: &str = "dev.dex.desktop";

/// Hides the console `reg.exe` would otherwise flash open.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The id toasts are shown under. Checked once: whether Dex's own id is
/// registered does not change while the app runs.
#[cfg(windows)]
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

#[cfg(windows)]
#[derive(Debug, Clone, Serialize)]
struct FocusPane {
    workspace: String,
    pane: String,
}

/// Shows a toast about the agent in `pane`; clicking it focuses that pane.
#[cfg(windows)]
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

/// Shows a Notification Center banner about the agent in `pane`. The title and
/// body go to AppleScript as arguments, never as script text, so nothing in
/// them can be run. macOS credits a banner shown this way to Script Editor,
/// and clicking it opens Script Editor, not Dex or the pane: a stopgap until
/// Dex posts its own notifications (#40). The helper is waited for on a
/// thread of its own, so none is left behind as a zombie.
#[cfg(not(windows))]
#[tauri::command]
#[allow(unused_variables)] // Tauri names the arguments; the banner needs only two.
pub fn notify_agent(
    app: AppHandle,
    title: String,
    body: String,
    workspace: String,
    pane: String,
) -> Result<(), String> {
    Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            &title,
            &body,
        ])
        .spawn()
        .map(|mut child| {
            std::thread::spawn(move || child.wait());
        })
        .map_err(|err| format!("could not show a notification: {err}"))
}
