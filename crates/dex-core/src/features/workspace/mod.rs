//! Workspaces, panes, and the pane layout tree.
//! Tables: `workspace`, `pane`, `app_state`.
//! Commands: `workspace.*` (and `pane.*` from M2).
//! Name/color/order rules and layout checks are pure and live in `logic.rs`.

mod commands;
mod logic;
mod model;
mod store;
#[cfg(test)]
mod tests;

pub use commands::{create, delete, list, recolor, rename, reorder, switch};
pub use model::WorkspaceError;
