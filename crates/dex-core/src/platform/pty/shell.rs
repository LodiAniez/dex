//! Default shell resolution: configured shell, then `pwsh.exe`, then
//! `powershell.exe`, then `cmd.exe` (docs/prd.md §7.1, §16.8).

use std::env;
use std::path::{Path, PathBuf};

/// Shells tried in order when none is configured, or the configured one is missing.
/// `pwsh` and `powershell` are different shells with different profiles; prefer `pwsh`.
const FALLBACKS: [&str; 3] = ["pwsh.exe", "powershell.exe", "cmd.exe"];

/// Returns the first available shell, or `None` if not even `cmd.exe` is found.
pub fn resolve_shell(configured: Option<&str>) -> Option<PathBuf> {
    configured
        .into_iter()
        .chain(FALLBACKS)
        .find_map(find_executable)
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
    fn falls_back_past_a_missing_configured_shell() {
        let shell = resolve_shell(Some("definitely-not-a-shell.exe")).unwrap();
        let name = shell.file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(FALLBACKS.contains(&name.as_str()), "resolved {shell:?}");
    }

    #[test]
    fn configured_absolute_path_wins_when_it_exists() {
        let cmd = find_executable("cmd.exe").unwrap();
        assert_eq!(resolve_shell(cmd.to_str()), Some(cmd));
    }
}
