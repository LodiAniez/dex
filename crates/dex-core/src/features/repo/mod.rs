//! Repository registry, git worktrees, and git status.
//! Tables: `repo`, `workspace_repo`.
//! Commands: `repo.*` (`commands.rs`), `worktree.*` (`worktree_commands.rs`).
//! Branch rules and git's porcelain formats are pure (`logic.rs`).
//! All git access goes through `platform::proc` running `git.exe`.

mod commands;
mod logic;
mod model;
mod store;
#[cfg(test)]
mod tests;
mod worktree_commands;

pub use commands::{WorkspaceRepo, add, diff, list, scan, status, workspace_repos};
pub use model::RepoError;
pub use worktree_commands::{
    add as add_worktree, attach_worktree, branch_at, create_for_spawn, list as list_worktrees,
    remove as remove_worktree,
};
