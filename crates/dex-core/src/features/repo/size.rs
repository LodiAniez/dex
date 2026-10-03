//! How much disk a checkout is holding.
//!
//! Asked for rather than always measured: a worktree with its dependencies
//! installed is tens of thousands of files, and walking them is seconds.
//!
//! The number is what the files say they are, and that is **not** what deleting
//! the folder gives back. A package store hardlinks one copy of a file into
//! every `node_modules` that wants it, so the same bytes are counted again in
//! each place they are found: thirty worktrees of one pnpm monorepo measured
//! 157 GB here, and deleting all thirty returned 4.3 GB, because the store they
//! all pointed at held 0.9 GB. Counting a link only once needs the file's
//! identity - its volume and index - which Rust offers on Unix (`ino`) but on
//! Windows only behind the nightly `windows_by_handle` feature, and the
//! alternative is an unsafe call into `GetFileInformationByHandle`, which this
//! codebase does not allow (conventions 4.1). So the number stays honest about
//! what it is: what the files claim, which is the right answer to "what is in
//! this worktree" and the wrong one to "what will I get back".

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
