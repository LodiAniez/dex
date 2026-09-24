//! How much disk a checkout is holding.
//!
//! Asked for rather than always measured: a worktree with its dependencies
//! installed is tens of thousands of files, and walking them is seconds. The
//! number is what the files say they are, which is not quite what deleting the
//! folder would give back - a file hardlinked from a package store (pnpm's, for
//! one) is counted here and is also still counted there - and it is the right
//! number for "how much is this worktree costing me".

use std::fs;
use std::path::{Path, PathBuf};

/// Bytes in `dir` and everything under it.
///
/// Symbolic links and Windows junctions are counted as the links they are and
/// never followed: `node_modules` is full of them, they point at folders
/// already counted, and one pointing upwards would walk the disk.
///
/// Anything unreadable is skipped rather than failing the measurement: this
/// number is for a human deciding what to delete, and "most of it" beats an
/// error because one file was open.
pub fn measure(dir: &Path) -> u64 {
    let mut total = 0;
    let mut left: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(next) = left.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                left.push(entry.path());
            } else if let Ok(file) = entry.metadata() {
                total += file.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::measure;

    #[test]
    fn the_size_is_every_file_under_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "12345").unwrap();
        let deep = dir.path().join("src").join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("b"), "123").unwrap();

        assert_eq!(measure(dir.path()), 8);
    }

    #[test]
    fn a_folder_that_is_not_there_holds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(measure(&dir.path().join("gone")), 0);
    }
}
