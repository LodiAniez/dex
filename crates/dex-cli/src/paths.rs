//! Where things are, as the CLI works them out without asking the app: the
//! data folder (token, config), the owner's home (Claude Code's settings and
//! skills), and the address the app listens on. Must match the daemon's copies
//! in `dex_core::platform::paths` and `dex_core::platform::pipe` - the CLI does
//! not depend on the daemon.

use std::ffi::OsString;
use std::path::PathBuf;

/// Where the running app can be reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// A Windows named pipe, by its full path (`\\.\pipe\dex-<user>`).
    Pipe(String),
    /// A Unix domain socket, by its path.
    Unix(PathBuf),
}

impl Address {
    /// What `dex doctor` calls it.
    pub fn describe(&self) -> String {
        match self {
            Self::Pipe(path) => format!("pipe {path}"),
            Self::Unix(path) => format!("socket {}", path.display()),
        }
    }
}

/// The data folder: `DEX_DATA_DIR` if set; else `%APPDATA%\Dex` on Windows,
/// `~/Library/Application Support/Dex` on macOS, `$XDG_DATA_HOME/Dex` (or
/// `~/.local/share/Dex`) elsewhere. `var` reads the environment.
pub fn data_dir_from(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    if let Some(dir) = var("DEX_DATA_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    #[cfg(windows)]
    {
        var("APPDATA").map(|base| PathBuf::from(base).join("Dex"))
    }
    #[cfg(target_os = "macos")]
    {
        var("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Dex")
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local").join("share")))
            .map(|base| base.join("Dex"))
    }
}

/// The data folder, from this process's environment.
pub fn data_dir() -> Option<PathBuf> {
    data_dir_from(|name| std::env::var_os(name))
}

/// The owner's home: `%USERPROFILE%` on Windows, `$HOME` elsewhere.
pub fn home_dir() -> Option<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(name).map(PathBuf::from)
}

/// The name the app listens under: `dex-<user>`, made safe for a path.
pub fn default_name(user: &str) -> String {
    let safe: String = user
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("dex-{safe}")
}

/// Where the app is: what `DEX_SOCKET` says (`pipe:<name>` or `unix:<path>`,
/// set in every Dex pane), else the default for this user on this platform.
pub fn address_from(var: impl Fn(&str) -> Option<OsString>) -> Option<Address> {
    let told = var("DEX_SOCKET").and_then(|value| value.into_string().ok());
    if let Some(told) = told {
        if let Some(name) = told.strip_prefix("pipe:").filter(|name| !name.is_empty()) {
            return Some(Address::Pipe(format!(r"\\.\pipe\{name}")));
        }
        if let Some(path) = told.strip_prefix("unix:").filter(|path| !path.is_empty()) {
            return Some(Address::Unix(PathBuf::from(path)));
        }
    }
    let user_var = if cfg!(windows) { "USERNAME" } else { "USER" };
    let user = var(user_var)
        .and_then(|user| user.into_string().ok())
        .unwrap_or_default();
    let name = default_name(&user);
    if cfg!(windows) {
        Some(Address::Pipe(format!(r"\\.\pipe\{name}")))
    } else {
        data_dir_from(var).map(|dir| Address::Unix(dir.join(format!("{name}.sock"))))
    }
}

/// Where the app is, from this process's environment.
pub fn address() -> Option<Address> {
    address_from(|name| std::env::var_os(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.into())
        }
    }

    #[test]
    fn a_pane_is_told_where_the_app_is_and_that_wins() {
        assert_eq!(
            address_from(env(&[("DEX_SOCKET", "pipe:dex-test")])),
            Some(Address::Pipe(r"\\.\pipe\dex-test".into()))
        );
        assert_eq!(
            address_from(env(&[("DEX_SOCKET", "unix:/tmp/dex/dex-me.sock")])),
            Some(Address::Unix("/tmp/dex/dex-me.sock".into()))
        );
    }

    #[test]
    fn names_are_per_user_and_safe_in_a_path() {
        assert_eq!(default_name("First Last"), "dex-first_last");
        assert_eq!(default_name(""), "dex-");
    }

    #[test]
    fn an_explicit_data_dir_wins_everywhere() {
        assert_eq!(
            data_dir_from(env(&[
                ("DEX_DATA_DIR", "/tmp/dex-a"),
                ("APPDATA", "C:/x"),
                ("HOME", "/Users/me")
            ])),
            Some(PathBuf::from("/tmp/dex-a"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_app_is_a_named_pipe_per_user() {
        assert_eq!(
            address_from(env(&[("USERNAME", "Admin")])),
            Some(Address::Pipe(r"\\.\pipe\dex-admin".into()))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_macos_the_app_is_a_socket_in_its_data_folder() {
        assert_eq!(
            address_from(env(&[("USER", "me"), ("HOME", "/Users/me")])),
            Some(Address::Unix(
                "/Users/me/Library/Application Support/Dex/dex-me.sock".into()
            ))
        );
    }
}
