//! Repository registry, git worktrees, and git status.
//! Tables: `repo`, `workspace_repo`.
//! Commands: `repo.*` (`commands.rs`), `worktree.*` (`worktree_commands.rs`),
//! `worktree.prune` (`prune.rs`), whose rules are pure (`verdict.rs`), which
//! asks the remote what has landed (`landed.rs`) and measures with `size.rs`.
//! Branch rules and git's porcelain formats are pure (`logic.rs`).
//! All git access goes through `platform::proc` running `git.exe`.

mod commands;
mod landed;
mod logic;
mod model;
mod placement;
mod prune;
mod size;
mod store;
#[cfg(test)]
mod tests;
mod verdict;
mod worktree_commands;

pub use commands::{WorkspaceRepo, add, diff, forget, list, scan, status, workspace_repos};
pub use model::RepoError;
pub use prune::prune as prune_worktrees;
pub use store::workspaces_using;
pub use worktree_commands::{
    add as add_worktree, attach_worktree, branch_at, create_for_spawn, list as list_worktrees,
    remove as remove_worktree,
};
