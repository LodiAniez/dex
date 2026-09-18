//! What a Mac app has to ask for that a Windows app is handed.
//!
//! An app started from the Finder or the Dock gets a bare environment: a
//! `PATH` of `/usr/bin:/bin:/usr/sbin:/sbin`, no terminal type, no locale.
//! Panes are fine - they run a login shell, which loads the owner's profile -
//! but whatever Dex runs *itself* (`git` for worktrees, `dex` for setup, which
//! runs `claude`) would not find tools installed through Homebrew or npm.
//! So Dex asks a login shell once for the owner's `PATH`, and hands it to the
//! programs it runs. On Windows none of this applies: apps inherit the owner's
//! `PATH`, and ConPTY sets the terminal type.

#[cfg(test)]
mod tests;

use std::ffi::OsString;

/// Printed by the login shell before its `PATH`, so a greeting from a profile
/// is not mistaken for part of it.
#[cfg(any(unix, test))]
const MARKER: &str = "__DEX_PATH__";

/// The owner's `PATH` as their login shell sets it, asked for once. `None` on
/// Windows, or if the shell could not be asked.
pub fn login_path() -> Option<&'static str> {
    #[cfg(unix)]
    {
        static PATH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        PATH.get_or_init(unix::ask_login_shell).as_deref()
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// What follows the marker on its line, if anything does.
#[cfg(any(unix, test))]
fn read_marked(printed: &str) -> Option<String> {
    let after = printed.split(MARKER).nth(1)?;
    let line = after.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// What a pane's environment needs that an app started from the Finder does
/// not have: a terminal type (the pane is an xterm.js terminal) and a UTF-8
/// locale, unless the owner has one of their own. `var` reads the app's
/// environment.
pub fn pane_defaults(var: impl Fn(&str) -> Option<OsString>) -> Vec<(String, String)> {
    let mut added = vec![
        ("TERM".to_owned(), "xterm-256color".to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
    ];
    if var("LANG").is_none() && var("LC_ALL").is_none() {
        added.push(("LANG".to_owned(), "en_US.UTF-8".to_owned()));
    }
    added
}

#[cfg(unix)]
mod unix {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{MARKER, read_marked};

    /// A profile that waits on something (an update prompt, a slow plugin)
    /// must not hold Dex's startup hostage.
    const ASK_WAIT: Duration = Duration::from_secs(5);

    pub(super) fn ask_login_shell() -> Option<String> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
        // Interactive as well as login: many people set PATH in .zshrc. Not in
        // a process group of its own: an interactive shell that finds itself
        // outside the foreground group gives up without printing anything.
        let mut child = Command::new(&shell)
            .args(["-ilc", &format!("printf '{MARKER}%s\\n'\"$PATH\"")])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        // Read as it comes: a profile that prints more than a pipe holds would
        // otherwise block the shell and look like a timeout.
        let mut stdout = child.stdout.take()?;
        let (sent, printed) = mpsc::channel();
        std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = stdout.read_to_end(&mut text);
            let _ = sent.send(text);
        });
        let Ok(text) = printed.recv_timeout(ASK_WAIT) else {
            let _ = child.kill();
            let _ = child.wait();
            tracing::warn!("{shell} did not say its PATH in time; tools keep the app's PATH");
            return None;
        };
        let _ = child.wait();
        let path = read_marked(&String::from_utf8_lossy(&text));
        if path.is_none() {
            tracing::warn!(
                "{shell} did not print its PATH (an unusual shell?); tools keep the app's PATH"
            );
        }
        path
    }
}
