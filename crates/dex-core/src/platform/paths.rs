//! Path normalization and the app's data directory.
//!
//! Every path stored in the database or sent over the wire is absolute and
//! uses forward slashes (docs/prd.md §5); convert at the boundary. WSL path
//! translation arrives with the features that need it.

use std::path::{Path, PathBuf};
use std::{env, fs, io};

/// `%APPDATA%\Dex`, created if missing. The database, config, and token live here.
pub fn app_data_dir() -> io::Result<PathBuf> {
    let base = env::var_os("APPDATA")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA is not set"))?;
    let dir = PathBuf::from(base).join("Dex");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The user's home directory (`%USERPROFILE%`).
pub fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE").map(PathBuf::from)
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

    #[test]
    fn normalize_leaves_forward_slash_paths_alone() {
        assert_eq!(normalize(Path::new("C:/src/api")), "C:/src/api");
    }
}
