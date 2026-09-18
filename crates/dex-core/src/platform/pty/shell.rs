//! Default shell resolution (docs/prd.md §7.1, §16.8).
//!
//! Windows: the configured shell, then `pwsh.exe`, `powershell.exe`, `cmd.exe`.
//! macOS and Linux: the configured shell, then the owner's `$SHELL`, then
//! `/bin/zsh`, `/bin/bash`, `/bin/sh` - started as a login shell (`shell_args`).

use std::env;
use std::path::{Path, PathBuf};

/// Shells tried in order when none is configured, or the configured one is missing.
/// `pwsh` and `powershell` are different shells with different profiles; prefer `pwsh`.
#[cfg(windows)]
const FALLBACKS: [&str; 3] = ["pwsh.exe", "powershell.exe", "cmd.exe"];
#[cfg(not(windows))]
const FALLBACKS: [&str; 3] = ["/bin/zsh", "/bin/bash", "/bin/sh"];

/// Returns the first available shell, or `None` if not even the last fallback is found.
pub fn resolve_shell(configured: Option<&str>) -> Option<PathBuf> {
    // The owner's own shell, where the platform has the idea.
    let own = if cfg!(unix) {
        env::var("SHELL").ok()
    } else {
        None
    };
    configured
        .map(str::to_owned)
        .into_iter()
        .chain(own)
        .chain(FALLBACKS.iter().map(|name| (*name).to_owned()))
        .find_map(|name| find_executable(&name))
}

/// Arguments a pane's shell starts with. On macOS and Linux a login shell, so
/// the owner's profile loads: an app started from the Finder has a bare
/// `PATH`, and `claude`, `git` and anything from Homebrew or npm would be
/// missing. Windows shells take no such flag.
pub fn shell_args(unix: bool) -> Vec<String> {
    if unix {
        vec!["-l".to_owned()]
    } else {
        Vec::new()
    }
}

/// Resolves `name` as an existing path, or by searching `PATH`.
fn find_executable(name: &str) -> Option<PathBuf> {
    let direct = Path::new(name);
    if direct.is_absolute() {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_shell_starts_as_a_login_shell_so_the_owners_path_and_profile_load() {
        // A Mac app started from the Finder gets a bare PATH; `claude`, `git`
        // and everything from Homebrew or npm is found through the profile.
        assert_eq!(shell_args(true), vec!["-l".to_owned()]);
        // Windows shells take no such flag.
        assert!(shell_args(false).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn falls_back_past_a_missing_configured_shell() {
        let shell = resolve_shell(Some("definitely-not-a-shell.exe")).unwrap();
        let name = shell.file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(FALLBACKS.contains(&name.as_str()), "resolved {shell:?}");
    }

    #[cfg(unix)]
    #[test]
    fn on_unix_the_owners_shell_is_used_and_there_is_always_one() {
        let shell = resolve_shell(Some("/definitely/not/a/shell")).unwrap();
        assert!(shell.is_absolute() && shell.is_file(), "{shell:?}");
    }

    #[cfg(windows)]
    #[test]
    fn configured_absolute_path_wins_when_it_exists() {
        let cmd = find_executable("cmd.exe").unwrap();
        assert_eq!(resolve_shell(cmd.to_str()), Some(cmd));
    }
}
