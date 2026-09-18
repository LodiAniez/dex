//! What a pane or a helper needs from the environment on a Mac.

use super::*;
use std::path::Path;

#[test]
fn a_pane_gets_a_terminal_type_and_a_utf8_locale_when_the_app_has_none() {
    // A Mac app started from the Finder has neither; zsh and Claude Code then
    // draw in plain ASCII.
    let added = pane_defaults(|_| None);
    assert!(added.contains(&("TERM".to_owned(), "xterm-256color".to_owned())));
    assert!(added.contains(&("COLORTERM".to_owned(), "truecolor".to_owned())));
    assert!(added.contains(&("LANG".to_owned(), "en_US.UTF-8".to_owned())));
}

#[test]
fn the_owners_own_locale_is_kept() {
    let added = pane_defaults(|name| (name == "LANG").then(|| "fr_FR.UTF-8".into()));
    assert!(!added.iter().any(|(name, _)| name == "LANG"), "{added:?}");
}

#[test]
fn the_path_a_login_shell_prints_is_read_back_exactly() {
    // The shell may print a greeting before it; the path is what follows the marker.
    let printed = "Welcome back!\n__DEX_PATH__/opt/homebrew/bin:/usr/bin:/bin\n";
    assert_eq!(
        read_marked(printed).as_deref(),
        Some("/opt/homebrew/bin:/usr/bin:/bin")
    );
    assert_eq!(read_marked("no marker here"), None);
    assert_eq!(read_marked("__DEX_PATH__\n"), None);
}

#[cfg(unix)]
#[test]
fn a_login_shell_gives_a_path_with_the_system_directories_in_it() {
    let path = login_path().expect("a login shell prints its PATH");
    assert!(path.split(':').any(|dir| dir == "/usr/bin"), "{path}");
}

#[cfg(windows)]
#[test]
fn windows_needs_none_of_it() {
    // Apps there inherit the owner's PATH, and ConPTY sets the terminal type.
    assert_eq!(login_path(), None);
}

#[test]
fn a_pane_finds_the_dex_beside_the_app_first() {
    // On macOS `dex` lives inside the app bundle, which no installer puts on
    // PATH; an agent in a pane runs `dex` by name.
    let dir = Path::new("/Applications/Dex.app/Contents/MacOS");
    assert_eq!(
        pane_path(dir, Some("/opt/homebrew/bin:/usr/bin")),
        "/Applications/Dex.app/Contents/MacOS:/opt/homebrew/bin:/usr/bin"
    );
    assert_eq!(pane_path(dir, None), "/Applications/Dex.app/Contents/MacOS");
    assert_eq!(
        pane_path(dir, Some("")),
        "/Applications/Dex.app/Contents/MacOS"
    );
}
