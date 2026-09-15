//! Path normalization and the app's data directory.
//!
//! Every path stored in the database or sent over the wire is absolute and
//! uses forward slashes (docs/prd.md §5); convert at the boundary. WSL path
//! translation and reserved-name checks arrive with the features that need them.

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

#[cfg(test)]
mod tests {
    use super::*;

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
