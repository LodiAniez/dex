//! Path normalization and the app's data directory.
//!
//! Every path stored in the database or sent over the wire is absolute and
//! uses forward slashes (docs/prd.md §5); convert at the boundary. WSL path
//! translation arrives with the features that need it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

/// Where the database, config, token and socket live, created if missing:
/// `%APPDATA%\Dex` on Windows, `~/Library/Application Support/Dex` on macOS,
/// `$XDG_DATA_HOME/Dex` (or `~/.local/share/Dex`) elsewhere. `DEX_DATA_DIR`
/// overrides it everywhere, for an isolated instance beside the owner's.
/// Must match the CLI's copy in `dex-cli/src/paths.rs`.
pub fn app_data_dir() -> io::Result<PathBuf> {
    let dir = data_dir_from(|name| env::var_os(name)).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no data directory: set DEX_DATA_DIR",
        )
    })?;
    private_dir(&dir)?;
    Ok(dir)
}

/// Creates `dir` if missing. On macOS and Linux it is then made owner-only,
/// since the token and the socket are in it; a folder someone else owns cannot
/// be, and Dex refuses to use it. On Windows the profile folder already is.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("{} must be Dex's own folder: {err}", dir.display()),
            )
        })?;
    }
    Ok(())
}

/// `app_data_dir` without creating it, reading the environment through `var`.
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

/// The user's home directory: `%USERPROFILE%` on Windows, `$HOME` elsewhere.
pub fn home_dir() -> Option<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(name).map(PathBuf::from)
}

/// The stored and wire form of a path: forward slashes only. Mixed separators
/// would otherwise leak into agent-written content and break comparisons.
pub fn normalize(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Names Windows refuses as a file or directory, with or without an extension
/// (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`), and names it
/// silently mangles (a trailing dot or space). Used for worktree branch names
/// (PRD §8) and context mirror keys (§10.1): both turn user text into paths.
pub fn is_reserved_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.to_ascii_uppercase();
    let numbered = |prefix: &str| {
        upper
            .strip_prefix(prefix)
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=9).contains(&n))
    };
    segment.is_empty()
        || segment.ends_with('.')
        || segment.ends_with(' ')
        || matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || numbered("COM")
        || numbered("LPT")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_names_are_rejected_with_and_without_extensions() {
        for name in ["CON", "con", "NUL.md", "com1", "LPT9.txt", "aux"] {
            assert!(is_reserved_name(name), "{name}");
        }
    }

    #[test]
    fn ordinary_names_are_allowed() {
        for name in ["console", "com0", "com10", "lpt", "notes", "auth-flow"] {
            assert!(!is_reserved_name(name), "{name}");
        }
    }

    #[test]
    fn windows_silently_strips_a_trailing_dot_or_space() {
        assert!(is_reserved_name("notes."));
        assert!(is_reserved_name("notes "));
        assert!(is_reserved_name(""));
    }

    #[test]
    fn normalize_replaces_every_backslash() {
        assert_eq!(
            normalize(Path::new(r"C:\Users\First Last\src")),
            "C:/Users/First Last/src"
        );
    }

    fn env<'a>(
        pairs: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Option<std::ffi::OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.into())
        }
    }

    #[test]
    fn an_explicit_data_dir_wins_everywhere() {
        // For isolated instances - a second Dex beside the owner's, and tests.
        let dir = data_dir_from(env(&[
            ("DEX_DATA_DIR", "/tmp/dex-a"),
            ("APPDATA", "C:/x"),
            ("HOME", "/Users/me"),
        ]));
        assert_eq!(dir, Some(PathBuf::from("/tmp/dex-a")));
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_data_lives_in_appdata() {
        let dir = data_dir_from(env(&[("APPDATA", r"C:\Users\me\AppData\Roaming")]));
        assert_eq!(dir, Some(PathBuf::from(r"C:\Users\me\AppData\Roaming\Dex")));
        assert_eq!(data_dir_from(env(&[])), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_macos_the_data_lives_in_application_support() {
        let dir = data_dir_from(env(&[("HOME", "/Users/me")]));
        assert_eq!(
            dir,
            Some(PathBuf::from("/Users/me/Library/Application Support/Dex"))
        );
        assert_eq!(data_dir_from(env(&[])), None);
    }

    #[test]
    fn normalize_leaves_forward_slash_paths_alone() {
        assert_eq!(normalize(Path::new("C:/src/api")), "C:/src/api");
    }

    #[cfg(unix)]
    #[test]
    fn the_data_folder_is_made_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("Dex");
        fs::create_dir(&data).unwrap();
        fs::set_permissions(&data, fs::Permissions::from_mode(0o755)).unwrap();
        private_dir(&data).unwrap();
        let mode = fs::metadata(&data).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}
