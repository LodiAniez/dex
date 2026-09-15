//! Workspaces, panes, and the pane layout tree.
//! Tables: `workspace`, `pane`, `app_state`.
//! Commands: `workspace.*` (`commands.rs`), `pane.*` and tree edits (`pane_commands.rs`).
//! Name/color/order rules (`logic.rs`) and tree operations (`layout.rs`) are pure.

mod commands;
mod layout;
mod logic;
mod model;
mod pane_commands;
mod store;
#[cfg(test)]
mod tests;

pub use commands::{create, delete, list, recolor, rename, reorder, switch};
pub use model::WorkspaceError;
pub use pane_commands::{close_pane, cycle_layout, focus_pane, set_layout, split_pane, swap_panes};
