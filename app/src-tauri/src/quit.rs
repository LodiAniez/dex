//! Quitting, and on macOS the app menu.
//!
//! Tauri's default Mac menu maps Cmd+W to Close Window, and closing the main
//! window quits Dex and every agent with it - one key from Cmd+Shift+W, which
//! closes a pane. So on macOS Dex sets its own menu: Quit (asking first while
//! panes are open), Hide, the Edit items that make Cmd+C and Cmd+V work in a
//! terminal, and the Window items - but no Close Window. The close button asks
//! the same question. Windows is unchanged: its caption buttons close as ever.

#[cfg(target_os = "macos")]
use tauri::Manager;
use tauri::{Window, WindowEvent};

use crate::popout;

/// What the quit question says, for `live` open panes.
#[cfg(any(target_os = "macos", test))]
fn quit_message(live: usize) -> String {
    let panes = if live == 1 {
        "1 pane"
    } else {
        &format!("{live} panes")
    };
    format!("{panes} will close, and everything running in them stops - agents included.")
}

/// Window events: the main window's close button asks first on macOS; closing
/// the main window quits, pop-outs and all.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    #[cfg(target_os = "macos")]
    if let WindowEvent::CloseRequested { api, .. } = event
        && window.label() == "main"
    {
        api.prevent_close();
        mac::confirm_quit(window.app_handle());
        return;
    }
    popout::quit_with_main(window, event);
}

#[cfg(target_os = "macos")]
pub use mac::install_menu;

#[cfg(target_os = "macos")]
mod mac {
    use dex_core::app::AppState;
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
    use tauri::{AppHandle, Manager};
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    use super::quit_message;

    const QUIT: &str = "quit";

    /// Replaces Tauri's default menu (see the module comment).
    pub fn install_menu(app: &AppHandle) -> tauri::Result<()> {
        let dex = Submenu::with_items(
            app,
            "Dex",
            true,
            &[
                &PredefinedMenuItem::about(app, Some("About Dex"), None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, None)?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::show_all(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, QUIT, "Quit Dex", true, Some("Cmd+Q"))?,
            ],
        )?;
        let edit = Submenu::with_items(
            app,
            "Edit",
            true,
            &[
                &PredefinedMenuItem::undo(app, None)?,
                &PredefinedMenuItem::redo(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::cut(app, None)?,
                &PredefinedMenuItem::copy(app, None)?,
                &PredefinedMenuItem::paste(app, None)?,
                &PredefinedMenuItem::select_all(app, None)?,
            ],
        )?;
        let window = Submenu::with_items(
            app,
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(app, None)?,
                &PredefinedMenuItem::maximize(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::fullscreen(app, None)?,
            ],
        )?;
        app.set_menu(Menu::with_items(app, &[&dex, &edit, &window])?)?;
        app.on_menu_event(|app, event| {
            if *event.id() == QUIT {
                confirm_quit(app);
            }
        });
        Ok(())
    }

    /// Quits, asking first if any pane is open.
    pub fn confirm_quit(app: &AppHandle) {
        let live = app.state::<AppState>().pty.live();
        if live == 0 {
            app.exit(0);
            return;
        }
        let handle = app.clone();
        app.dialog()
            .message(quit_message(live))
            .title("Quit Dex?")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Quit".into(),
                "Cancel".into(),
            ))
            .show(move |quit| {
                if quit {
                    handle.exit(0);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quit_question_says_how_much_will_stop() {
        assert_eq!(
            quit_message(1),
            "1 pane will close, and everything running in them stops - agents included."
        );
        assert!(quit_message(4).starts_with("4 panes will close"));
    }
}
