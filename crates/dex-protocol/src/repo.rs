//! Wire types for the repository registry and git worktrees (docs/prd.md §8).

use serde::{Deserialize, Serialize};

/// A registered repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RepoView {
    /// Repo id.
    pub id: String,
    /// Display name; defaults to the directory name.
    pub name: String,
    /// Absolute path, forward slashes.
    pub path: String,
    /// Checked-out branch of the main checkout, if git could say.
    pub branch: Option<String>,
}

/// Result of `repo.list`, `repo.add`, and `repo.scan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RepoList {
    /// Repos, by name.
    pub repos: Vec<RepoView>,
}

/// Working-tree counts against HEAD (PRD §8), for the status bar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RepoStatus {
    /// Files added or untracked.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub added: usize,
    /// Files modified.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub modified: usize,
    /// Files deleted.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub deleted: usize,
    /// The branch checked out where this was measured.
    pub branch: Option<String>,
}

/// Args for `repo.add`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddRepoArgs {
    /// Absolute path of a git repository.
    pub path: String,
    /// Display name; the directory name by default.
    #[serde(default)]
    pub name: Option<String>,
}

/// Args for `repo.scan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanArgs {
    /// Directory to search.
    pub path: String,
    /// How deep to look; 3 by default.
    #[serde(default)]
    pub depth: Option<u32>,
}

/// Args for `repo.status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoArgs {
    /// Repo name or id.
    pub repo: String,
}

/// Args for `worktree.add`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddWorktreeArgs {
    /// Repo name or id.
    pub repo: String,
    /// Branch to create and check out.
    pub branch: String,
    /// Attach the worktree to this workspace (id or name).
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Args for `worktree.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveWorktreeArgs {
    /// Repo name or id.
    pub repo: String,
    /// Branch whose worktree goes.
    pub branch: String,
    /// Remove it even when it holds uncommitted work.
    #[serde(default)]
    pub force: bool,
}

/// A worktree on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WorktreeView {
    /// Absolute path, forward slashes.
    pub path: String,
    /// Branch checked out there, absent when detached.
    pub branch: Option<String>,
    /// Whether this is the repo's main checkout rather than a worktree.
    pub main: bool,
}

/// Result of `worktree.add`, `worktree.remove`, and `worktree.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WorktreeList {
    /// Every checkout git knows for the repo, main first.
    pub worktrees: Vec<WorktreeView>,
}

/// Args for `repo.diff`: the working-tree or staged diff of the repository
/// holding `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffArgs {
    /// A directory inside the repository, forward slashes.
    pub path: String,
    /// `git diff --cached` rather than `git diff`.
    #[serde(default)]
    pub staged: bool,
}

/// Result of `repo.diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RepoDiff {
    /// The unified diff, empty when there are no changes.
    pub text: String,
    /// Whether `text` was cut short.
    pub truncated: bool,
    /// The checked-out branch, if on one.
    pub branch: Option<String>,
}
