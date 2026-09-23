//! Where a worktree goes (PRD §8), and making room for it there.
//!
//! Inside the workspace it is for: `<root>/.dex/worktrees/<repo>/<branch>`,
//! beside the context mirror, at the owner's request - an agent's checkout
//! belongs with the work it is part of. Only a spawn with no workspace at all,
//! or one Windows git could not then use, falls back to `worktree_base`.
//!
//! A root is often the repository itself, so the worktree usually sits inside
//! a checkout of its own repo, and that is known and accepted:
//!
//! - Git is told to look away twice: in `.dex/worktrees/.gitignore`, and in
//!   the containing repository's `info/exclude`, which is the one `git clean`
//!   cannot delete. So the worktrees never show in that checkout's
//!   `git status` and are never taken into a `git add -A` as embedded
//!   repositories. A root whose repository will not take the second is not
//!   used at all: the worktree goes to `worktree_base`.
//! - `git clean -ffdx` in that checkout still deletes them, uncommitted work
//!   and all - no ignore or lock stops a second `-f` - so the folder carries a
//!   note saying so.
//! - Anything that looks up the folder tree finds the project around the
//!   worktree: Claude Code loads every `CLAUDE.md` above it, Node resolves
//!   modules upwards, Cargo walks up to a `[workspace]`, ESLint to its config.

use std::path::{Path, PathBuf};

use super::logic;
use super::model::RepoError;
use crate::platform::paths;
use crate::platform::proc;

/// Which git makes the worktree: Windows', or a WSL distro's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadeBy {
    Windows,
    Wsl,
}

/// Left in the worktree folder for whoever finds it: git is quiet about it, so
/// nothing else says what is in there.
const NOTE: &str = "\
Dex keeps agents' git worktrees here, one folder per repository and branch.
They are working checkouts with work in them that may not be committed yet.
Git is told to ignore this folder, so `git clean -ffdx` in the repository
above will delete all of it without asking. `dex worktree list <repo>` shows
what is here, and `dex worktree remove <repo> <branch>` takes one away.
";

/// Where the worktree for `branch` of `repo` goes, with room made for it:
/// inside the workspace rooted at `root`, fenced off from the git around it,
/// and under `outside` - the owner's `worktree_base` - when there is no
/// workspace, or the root cannot hold it.
pub fn place(
    outside: &Path,
    root: Option<&str>,
    made_by: MadeBy,
    repo: &str,
    branch: &str,
) -> PathBuf {
    let base = match inside(root, made_by) {
        Some(root) => {
            let base = logic::worktree_base(outside, Some(root.as_path()));
            match fence_off(&base, &root) {
                Ok(()) => base,
                Err(err) => {
                    tracing::warn!(%err, "the workspace cannot hold its worktrees; using worktree_base");
                    outside.to_path_buf()
                }
            }
        }
        None => outside.to_path_buf(),
    };
    logic::worktree_path(&base, repo, branch)
}

/// The workspace's root, when the worktree can go inside it: an existing
/// folder the git making the worktree can work in.
fn inside(root: Option<&str>, made_by: MadeBy) -> Option<PathBuf> {
    let root = Path::new(root?);
    // Windows git cannot work in a worktree on the distro's side of
    // `\\wsl.localhost`: to it, a folder there is not local and not its own.
    if made_by == MadeBy::Windows && in_wsl(root) {
        return None;
    }
    if !root.is_dir() {
        return None;
    }
    // A bare workspace's root is the real home; a test must give its own.
    #[cfg(test)]
    assert!(
        paths::home_dir().as_deref() != Some(root),
        "a test was about to put a worktree in the real home"
    );
    Some(root.to_path_buf())
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

/// Makes the workspace's worktree folder, tells the git around it to look away
/// from it, and leaves the note saying what is in there.
///
/// Twice, because a `.gitignore` inside the folder is itself ignored and so
/// `git clean -fdx` deletes it, after which `git add -A` would take a worktree
/// in as an embedded repository: the `info/exclude` of the repository holding
/// the root says the same thing where clean cannot reach. A `.gitignore`
/// already there is the owner's, and is left as it is.
fn fence_off(base: &Path, root: &Path) -> Result<(), RepoError> {
    if let Some(exclude) = exclude_file(root) {
        // First, and fatal: the fence inside the folder cannot be relied on -
        // `git clean -fdx` deletes it, being ignored by its own rule - so a
        // worktree that cannot be excluded is a worktree that does not go
        // here. `place` sends it to `worktree_base` instead.
        tell_git_to_ignore_dex(&exclude)?;
    }
    make_dir(base)?;
    write_once(&base.join(".gitignore"), logic::IGNORE_EVERYTHING)?;
    write_once(&base.join("README.txt"), NOTE)?;
    Ok(())
}

/// Appends `logic::EXCLUDE_DEX` to a repository's `info/exclude`, once.
///
/// Appended rather than rewritten: the file is the owner's, and another Dex
/// making a worktree at the same moment - or the owner in an editor - must not
/// have their line truncated away. Two appends at once would leave the line
/// twice, which git reads as once.
fn tell_git_to_ignore_dex(exclude: &Path) -> Result<(), RepoError> {
    let told = |err: std::io::Error| RepoError::WorktreeDir {
        path: paths::normalize(exclude),
        reason: err.to_string(),
    };
    let current = std::fs::read_to_string(exclude).unwrap_or_default();
    if current.lines().any(|had| had.trim() == logic::EXCLUDE_DEX) {
        return Ok(());
    }
    if let Some(info) = exclude.parent() {
        // `info/` is missing from a repository made with an empty template.
        std::fs::create_dir_all(info).map_err(told)?;
    }
    let ending = if current.is_empty() || current.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(exclude)
        .map_err(told)?;
    std::io::Write::write_all(
        &mut file,
        format!("{ending}{}\n", logic::EXCLUDE_DEX).as_bytes(),
    )
    .map_err(told)
}

/// Writes a file the first time only: what is there afterwards is the owner's.
fn write_once(path: &Path, text: &str) -> Result<(), RepoError> {
    if path.exists() {
        return Ok(());
    }
    std::fs::write(path, text).map_err(|err| RepoError::WorktreeDir {
        path: paths::normalize(path),
        reason: err.to_string(),
    })
}

/// `info/exclude` of the repository whose checkout holds `root`, if one does.
/// The common directory, so a root that is itself in a worktree reaches the
/// repository every checkout of it shares.
fn exclude_file(root: &Path) -> Option<PathBuf> {
    let printed = proc::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok()?
    .stdout;
    let git_dir = Path::new(printed.trim());
    git_dir
        .is_dir()
        .then(|| git_dir.join("info").join("exclude"))
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

    use super::{NOTE, in_wsl, same_place};

    #[test]
    fn a_folder_through_wsl_localhost_is_on_the_distros_side() {
        assert!(in_wsl(Path::new(r"\\wsl.localhost\Ubuntu\home\me\code")));
        assert!(in_wsl(Path::new("//wsl$/Ubuntu/home/me")));
        assert!(in_wsl(Path::new(r"\\?\UNC\wsl.localhost\Ubuntu\home")));
        assert!(!in_wsl(Path::new("C:/code")));
    }

    #[test]
    fn a_root_windows_git_cannot_work_in_sends_the_worktree_outside() {
        let outside = Path::new("C:/Users/me/dex/worktrees");
        let wsl = "//wsl.localhost/Ubuntu/home/me/code";
        assert_eq!(
            super::place(
                outside,
                Some(wsl),
                super::MadeBy::Windows,
                "api",
                "fix/login"
            ),
            outside.join("api").join("fix-login")
        );
    }

    #[test]
    fn the_note_says_what_deletes_the_worktrees() {
        assert!(NOTE.contains("git clean -ffdx"));
        assert!(NOTE.contains("dex worktree list"));
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
