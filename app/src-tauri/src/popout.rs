//! A pane in a window of its own (after issue #31). The window is a second
//! webview of the same app, told which pane it shows through an init script;
//! the terminal moves into it by the hand-off in `pty.rs` (`pty_hold`,
//! `pty_attach`), driven from the frontend. When it goes - docked, closed, or
//! crashed - the main window is told, so the pane comes home either way.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Told to the main window when a pane's window has gone, however it went.
const CLOSED_EVENT: &str = "popout-closed";

/// What the pane's window needs to know before its page loads.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Popout<'a> {
    pane_id: &'a str,
    title: &'a str,
}

/// The window label for a pane: `pane-<id>`, which the pop-out capability matches.
fn label_for(pane_id: &str) -> String {
    format!("pane-{pane_id}")
}

/// Opens `pane_id` in a window of its own, or brings its window forward if it
/// already has one.
/// Async on purpose: a synchronous command that builds a window deadlocks on
/// Windows, leaving it blank.
#[tauri::command]
pub async fn pane_pop_out(app: AppHandle, pane_id: String, title: String) -> Result<(), String> {
    let label = label_for(&pane_id);
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.unminimize();
        return existing.set_focus().map_err(|err| err.to_string());
    }
    let who = serde_json::to_string(&Popout {
        pane_id: &pane_id,
        title: &title,
    })
    .map_err(|err| err.to_string())?;
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title(format!("{title} - Dex"))
        .inner_size(900.0, 560.0)
        .min_inner_size(360.0, 200.0)
        .initialization_script(format!("window.__DEX_POPOUT__ = {who};"))
        .build()
        .map_err(|err| err.to_string())?;
    let home = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            let _ = home.emit_to("main", CLOSED_EVENT, &pane_id);
        }
    });
    Ok(())
}

/// Closing the main window quits Dex, pop-outs and all: a pane's window with
/// no main window behind it would be a terminal nobody can dock or manage.
pub fn quit_with_main(window: &tauri::Window, event: &WindowEvent) {
    if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
        window.app_handle().exit(0);
    }
}
