//! Where a worktree goes (PRD §8), and making room for it there.
//!
//! A worktree exists to be a place of its own: an agent working in one must
//! reach nothing outside it. So it goes inside its workspace only when the
//! workspace's root is a plain folder; a root inside a git checkout would put
//! the worktree below that project's files, where everything that looks up
//! the folder tree finds them (`logic::worktree_base` has the rest), and there
//! it goes to the owner's `worktree_base` instead.

use std::path::{Path, PathBuf};

use super::logic;
use super::model::RepoError;
use crate::platform::paths;
use crate::platform::proc::{self, GitError};

/// Which git makes the worktree: Windows', or a WSL distro's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadeBy {
    Windows,
    Wsl,
}

/// Where the worktree for `branch` of `repo` goes, with room made for it:
/// inside the workspace rooted at `root` when that is a plain folder, fenced
/// off from git, and otherwise under `outside`, the owner's `worktree_base`.
/// A plain root that cannot take the folder sends the worktree outside too:
/// never wrong, only further away.
pub fn place(
    outside: &Path,
    root: Option<&str>,
    made_by: MadeBy,
    repo: &str,
    branch: &str,
) -> PathBuf {
    let inside =
        plain_root(root, made_by).map(|root| logic::worktree_base(outside, Some(root.as_path())));
    let base = match inside {
        Some(base) => match fence_off(&base) {
            Ok(()) => base,
            Err(err) => {
                tracing::warn!(%err, "the workspace cannot hold its worktrees; using worktree_base");
                outside.to_path_buf()
            }
        },
        None => outside.to_path_buf(),
    };
    logic::worktree_path(&base, repo, branch)
}

/// The workspace's root, when a worktree may go inside it: an existing folder
/// that no git checkout contains, which the git making the worktree can use.
///
/// The folders are asked as well as git. Git alone says "not a git repository"
/// of a folder that plainly is in one whenever it stops looking too soon - a
/// stale `.git` file left by a moved worktree, a broken repository,
/// `GIT_CEILING_DIRECTORIES`, a mount boundary - and a `.git` in the root or
/// any folder above it settles that. Git is still asked, for what has no
/// `.git` entry (a bare repository), and anything it cannot answer plainly -
/// not installed, a repository it will not read - counts as a checkout: a
/// worktree kept outside is never wrong, only further away. This is Windows
/// git even for a spawn into WSL; without it, the root counts as a checkout.
fn plain_root(root: Option<&str>, made_by: MadeBy) -> Option<PathBuf> {
    let root = Path::new(root?);
    if !root.is_dir() {
        return None;
    }
    // A bare workspace's root is the real home; a test must give its own.
    #[cfg(test)]
    assert!(
        paths::home_dir().as_deref() != Some(root),
        "a test was about to put a worktree in the real home"
    );
    // Windows git will not work in a worktree on the distro's side of
    // `\\wsl.localhost`: to it, a folder there is not local and not its own.
    if made_by == MadeBy::Windows && in_wsl(root) {
        return None;
    }
    if root.ancestors().any(|dir| dir.join(".git").exists()) {
        return None;
    }
    match proc::git(root, &["rev-parse", "--is-inside-work-tree"]) {
        Err(GitError::NotARepo(_)) => Some(root.to_path_buf()),
        _ => None,
    }
}

/// Whether a folder is on a WSL distro's side, through `\\wsl.localhost` or
/// `\\wsl$` - as written, or as the file system resolves it, which catches a
/// drive letter mapped to one.
fn in_wsl(dir: &Path) -> bool {
    let through_wsl = |path: &Path| {
        let normal = paths::normalize(path).to_ascii_lowercase();
        let normal = normal
            .strip_prefix("//?/unc/")
            .map_or(normal.clone(), |rest| format!("//{rest}"));
        normal.starts_with("//wsl.localhost/") || normal.starts_with("//wsl$/")
    };
    through_wsl(dir) || std::fs::canonicalize(dir).is_ok_and(|real| through_wsl(&real))
}

/// Makes a workspace's worktree folder and tells git to look away from it
/// (`logic::IGNORE_EVERYTHING`). A `.gitignore` already there is the owner's,
/// and is left as it is.
fn fence_off(base: &Path) -> Result<(), RepoError> {
    make_dir(base)?;
    let ignore = base.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, logic::IGNORE_EVERYTHING).map_err(|err| {
            RepoError::WorktreeDir {
                path: paths::normalize(&ignore),
                reason: err.to_string(),
            }
        })?;
    }
    Ok(())
}

/// `create_dir_all`, failing as the folder it could not make.
pub fn make_dir(dir: &Path) -> Result<(), RepoError> {
    std::fs::create_dir_all(dir).map_err(|err| RepoError::WorktreeDir {
        path: paths::normalize(dir),
        reason: err.to_string(),
    })
}

/// Whether two paths name the same folder: as the file system resolves them
/// where both exist - a root typed `c:\code\.` and git's `C:/code` are one
/// place - and as written otherwise, ignoring case on Windows.
pub fn same_place(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return a == b;
    }
    let (a, b) = (paths::normalize(a), paths::normalize(b));
    if cfg!(windows) {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{in_wsl, same_place};

    #[test]
    fn a_folder_through_wsl_localhost_is_on_the_distros_side() {
        assert!(in_wsl(Path::new(r"\\wsl.localhost\Ubuntu\home\me\code")));
        assert!(in_wsl(Path::new("//wsl$/Ubuntu/home/me")));
        assert!(in_wsl(Path::new(r"\\?\UNC\wsl.localhost\Ubuntu\home")));
        assert!(!in_wsl(Path::new("C:/code")));
    }

    #[test]
    fn one_folder_written_two_ways_is_one_place() {
        let dir = tempfile::tempdir().unwrap();
        let written = format!("{}/.", dir.path().to_string_lossy());
        assert!(same_place(dir.path(), Path::new(&written)));
        assert!(!same_place(dir.path(), &dir.path().join("elsewhere")));
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_a_folder_that_is_gone_is_matched_ignoring_case() {
        assert!(same_place(
            Path::new("C:/Gone/Worktrees/api"),
            Path::new("c:/gone/worktrees/API")
        ));
    }
}
