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

/// Args for `worktree.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListWorktreesArgs {
    /// Repo name or id.
    pub repo: String,
    /// Measure each checkout on disk. Off by default: it walks every file in
    /// every worktree, which for a checkout with dependencies installed is
    /// seconds, not milliseconds.
    #[serde(default)]
    pub sizes: bool,
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
    /// Bytes it holds, when that was asked for and could be measured.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub size_bytes: Option<i64>,
}

/// Result of `worktree.add`, `worktree.remove`, and `worktree.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WorktreeList {
    /// Every checkout git knows for the repo, main first.
    pub worktrees: Vec<WorktreeView>,
}

/// Args for `worktree.prune`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruneWorktreesArgs {
    /// Repo name or id.
    pub repo: String,
    /// Say what would go without taking anything away.
    #[serde(default)]
    pub dry_run: bool,
}

/// Why a worktree was left where it is. Every one of these is a reason to
/// believe the worktree may still hold work, or that Dex could not tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum KeptBecause {
    /// A pane whose shell is running has its folder inside it.
    InUse,
    /// Its working tree has changes git has not been told to keep.
    Uncommitted,
    /// Its branch has commits the main checkout's branch does not.
    Unmerged,
    /// No branch is checked out, so there is nothing to compare.
    Detached,
    /// Git could not say how its branch compares, so Dex will not guess.
    Unknown,
    /// It was safe to take away and git refused to.
    Refused,
}

/// A worktree left where it is, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct KeptWorktree {
    /// The worktree.
    pub worktree: WorktreeView,
    /// What kept it.
    pub because: KeptBecause,
}

/// Result of `worktree.prune`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Pruned {
    /// The worktrees taken away, each with what it held. With `dry_run` these
    /// are still there: they are what a prune would take.
    pub taken: Vec<WorktreeView>,
    /// The worktrees left, each with the reason it stayed.
    pub kept: Vec<KeptWorktree>,
    /// Bytes the taken worktrees held, measured before they went.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub freed_bytes: i64,
    /// Whether this was a dry run, so nothing was actually removed.
    pub dry_run: bool,
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
