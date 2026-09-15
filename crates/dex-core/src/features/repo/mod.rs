//! Repository registry, git worktrees, and git status.
//! Tables: `repo`, `workspace_repo`.
//! Commands: `repo.*`, `worktree.*`.
//! All git access goes through `platform::proc` running `git.exe`.

mod commands;
mod logic;
mod model;
mod store;
#[cfg(test)]
mod tests;
