//! Workspaces, panes, and the pane layout tree.
//! Tables: `workspace`, `pane`, `app_state`.
//! Commands: `workspace.*` (`commands.rs`); `pane.*` tree edits (`pane_commands.rs`);
//! `pane.list/send/send_key` (`pane_io.rs`). Rules and tree operations are pure (`logic.rs`, `layout.rs`).

mod commands;
mod layout;
mod logic;
mod model;
mod pane_commands;
mod pane_io;
mod store;
mod targets;
#[cfg(test)]
mod tests;

pub use commands::{create, delete, list, recolor, rename, reorder, switch};
pub use model::WorkspaceError;
pub use pane_commands::{
    close_pane, create_pane, cycle_layout, focus_pane, label_pane, set_layout, split_pane,
    swap_panes,
};
pub use pane_io::{list_panes, send, send_key};
