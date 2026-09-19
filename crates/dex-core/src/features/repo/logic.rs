//! Pure rules for repos and worktrees: what a branch may be called, where its
//! worktree lands, and how to read git's porcelain output (docs/prd.md §8).
//! No I/O, no database, no subprocesses.

use std::path::{Path, PathBuf};

use crate::platform::paths;

/// Why a branch name cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadBranch {
    Empty,
    /// A character git itself refuses, or one that would break a path.
    Character,
    /// A segment Windows cannot store under that name.
    Reserved,
}

impl BadBranch {
    /// What the caller should do instead.
    pub fn repair(self) -> &'static str {
        match self {
            Self::Empty => "Give a branch name, such as `fix/login`.",
            Self::Character => {
                "Use letters, digits, and `. _ - /` in the branch name; git refuses the rest."
            }
            Self::Reserved => {
                "Rename that part of the branch: Windows reserves names like `con` and `nul`."
            }
        }
    }
}

/// Checks a branch name against git's rules and what Windows can store.
///
/// The name becomes a directory under the worktree base, so this is the same
/// kind of boundary as a context key: a name that escapes the base directory
/// would put a checkout somewhere nobody expects.
pub fn check_branch(branch: &str) -> Result<(), BadBranch> {
    if branch.is_empty() {
        return Err(BadBranch::Empty);
    }
    // git refuses these outright (`git check-ref-format`), and the rest would
    // not survive becoming a path.
    let illegal = |c: char| {
        c.is_control()
            || matches!(
                c,
                ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '"' | '\'' | '|' | '<' | '>'
            )
    };
    if branch.chars().any(illegal)
        || branch.contains("..")
        || branch.contains("@{")
        || branch.starts_with('/')
        || branch.starts_with('-')
        || branch.ends_with('/')
        || branch.ends_with(".lock")
    {
        return Err(BadBranch::Character);
    }
    for segment in branch.split('/') {
        if segment.is_empty() || segment == "." {
            return Err(BadBranch::Character);
        }
        if paths::is_reserved_name(segment) {
            return Err(BadBranch::Reserved);
        }
    }
    Ok(())
}

/// The directory name a branch gets: slashes become dashes, so `fix/login` is
/// one directory rather than a nested pair (PRD §8).
pub fn branch_dir(branch: &str) -> String {
    branch.replace('/', "-")
}

/// Where a worktree lives: `<base>/<repo>/<branch with dashes>`.
pub fn worktree_path(base: &Path, repo: &str, branch: &str) -> PathBuf {
    base.join(repo).join(branch_dir(branch))
}

/// A repo's default display name: the directory it sits in.
pub fn repo_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "repo".into())
}

/// What `git status --porcelain=v1` says, counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Status {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
}

/// Counts a porcelain v1 listing.
///
/// Each line is `XY path`, where X is the staged state and Y the unstaged one.
/// A file counts once, by the more definite of the two, so a staged rename that
/// is then edited does not inflate the numbers.
pub fn parse_status(porcelain: &str) -> Status {
    let mut status = Status::default();
    for line in porcelain.lines() {
        if line.len() < 2 {
            continue;
        }
        let code: Vec<char> = line.chars().take(2).collect();
        match (code[0], code[1]) {
            ('?', '?') | ('A', _) => status.added += 1,
            ('D', _) | (_, 'D') => status.deleted += 1,
            (' ', ' ') => {}
            _ => status.modified += 1,
        }
    }
    status
}

/// The branch and path of every worktree in `git worktree list --porcelain`.
///
/// A detached worktree has no branch; it is reported with `None` rather than
/// skipped, because it still occupies its path.
pub fn parse_worktrees(porcelain: &str) -> Vec<(String, Option<String>)> {
    let mut found = Vec::new();
    let mut path: Option<String> = None;
    let mut branch: Option<String> = None;
    for line in porcelain.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(previous) = path.take() {
                found.push((previous, branch.take()));
            }
            path = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = Some(short_ref(rest.trim()));
        }
    }
    if let Some(last) = path {
        found.push((last, branch));
    }
    found
}

/// `refs/heads/fix/login` as `fix/login`.
pub fn short_ref(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_owned()
}

/// The first git that makes worktrees with `--relative-paths`, and reads a
/// repository that has one (it marks the repository's format for it).
pub const RELATIVE_WORKTREES: (u32, u32) = (2, 48);

/// Major and minor from `git --version`: `git version 2.53.0`, or Git for
/// Windows' `git version 2.55.0.windows.5`.
pub fn git_version(printed: &str) -> Option<(u32, u32)> {
    let number = printed.trim().strip_prefix("git version ")?;
    let mut parts = number.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_git_version_is_read_from_either_side() {
        assert_eq!(
            super::git_version(
                "git version 2.53.0
"
            ),
            Some((2, 53))
        );
        assert_eq!(
            super::git_version("git version 2.55.0.windows.5"),
            Some((2, 55))
        );
        assert_eq!(super::git_version("git version 2.43.0"), Some((2, 43)));
        assert!(super::git_version("git version 2.43.0").unwrap() < super::RELATIVE_WORKTREES);
        assert_eq!(super::git_version("bash: git: command not found"), None);
    }

    use super::*;

    #[test]
    fn ordinary_branch_names_are_accepted() {
        for branch in ["main", "fix/login", "release-1.2", "feat_x", "a/b/c"] {
            assert_eq!(check_branch(branch), Ok(()), "{branch}");
        }
    }

    #[test]
    fn names_git_itself_refuses_are_refused() {
        for branch in [
            "has space",
            "tilde~1",
            "caret^",
            "colon:name",
            "star*",
            "question?",
            "back\\slash",
            "dots..name",
            "at@{x}",
            "/leading",
            "trailing/",
            "-leading-dash",
            "thing.lock",
            "a//b",
        ] {
            assert_eq!(check_branch(branch), Err(BadBranch::Character), "{branch}");
        }
        assert_eq!(check_branch(""), Err(BadBranch::Empty));
    }

    #[test]
    fn branches_windows_cannot_store_are_refused() {
        assert_eq!(check_branch("con"), Err(BadBranch::Reserved));
        assert_eq!(check_branch("fix/nul"), Err(BadBranch::Reserved));
        assert_eq!(check_branch("com1"), Err(BadBranch::Reserved));
    }

    #[test]
    fn every_refusal_says_what_to_do_instead() {
        for bad in [BadBranch::Empty, BadBranch::Character, BadBranch::Reserved] {
            assert!(!bad.repair().is_empty(), "{bad:?}");
        }
    }

    #[test]
    fn a_branch_becomes_one_directory_under_its_repo() {
        assert_eq!(
            worktree_path(Path::new("C:/wt"), "api", "fix/login"),
            Path::new("C:/wt/api/fix-login")
        );
        assert_eq!(branch_dir("a/b/c"), "a-b-c");
        assert_eq!(branch_dir("main"), "main");
    }

    #[test]
    fn a_repo_is_named_after_its_directory() {
        assert_eq!(repo_name(Path::new("C:/src/api")), "api");
        assert_eq!(repo_name(Path::new("C:/src/api/")), "api");
    }

    #[test]
    fn porcelain_status_is_counted_by_file() {
        let listing = "\
 M src/main.rs
A  src/new.rs
?? notes.md
 D gone.rs
MM both.rs
";
        let status = parse_status(listing);
        assert_eq!(status.added, 2, "one staged add and one untracked file");
        assert_eq!(status.deleted, 1);
        assert_eq!(status.modified, 2);
        assert_eq!(
            parse_status(""),
            Status::default(),
            "a clean tree counts zero"
        );
    }

    #[test]
    fn worktrees_are_read_with_their_branches() {
        let listing = "\
worktree C:/src/api
HEAD abc123
branch refs/heads/main

worktree C:/wt/api/fix-login
HEAD def456
branch refs/heads/fix/login

worktree C:/wt/api/detached
HEAD 999aaa
detached
";
        let found = parse_worktrees(listing);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0], ("C:/src/api".into(), Some("main".into())));
        assert_eq!(
            found[1],
            ("C:/wt/api/fix-login".into(), Some("fix/login".into()))
        );
        assert_eq!(
            found[2],
            ("C:/wt/api/detached".into(), None),
            "a detached worktree still occupies its path"
        );
    }

    #[test]
    fn a_ref_is_shortened_to_its_branch() {
        assert_eq!(short_ref("refs/heads/fix/login"), "fix/login");
        assert_eq!(short_ref("main"), "main");
    }
}
