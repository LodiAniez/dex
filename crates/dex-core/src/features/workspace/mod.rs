//! Workspaces, panes, and the pane layout tree.
//! Tables: `workspace`, `pane`, `app_state`.
//! Commands: `workspace.*` (`commands.rs`); `pane.*` tree edits (`pane_commands.rs`);
//! `pane.list/send/send_key` (`pane_io.rs`); `pane.content` (`pane_content.rs`);
//! `pane.terminal`, the terminal every new pane opens in (`runtimes.rs`).
//! Rules and tree operations are pure (`logic.rs`, `layout.rs`).

mod arrange;
mod commands;
pub(crate) mod docking;
mod layout;
mod logic;
mod model;
mod pane_commands;
mod pane_content;
mod pane_io;
mod pane_move;
mod runtimes;
mod store;
mod switching;
mod targets;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use arrange::preset_layout;
pub use arrange::{arrange, preset_in_use};
pub use commands::{create, delete, list, recolor, rename, reorder, switch};
#[cfg(test)]
pub use layout::Preset;
#[cfg(test)]
pub use layout::leaves as leaves_of;
pub use model::WorkspaceError;
pub use pane_commands::{
    close_pane, create_pane, cycle_layout, focus_pane, label_pane, set_layout, split_pane,
    split_pane_in, swap_panes,
};
pub use pane_content::content as pane_content;
pub use pane_io::{list_panes, send, send_key};
pub use pane_move::move_pane;
pub use runtimes::{choose_terminal, open_panes_in_terminal, terminal};
#[cfg(test)]
pub use store::update_terminal as set_terminal;
pub use store::{
    find_pane_workspace, pane_cwd, pane_label, pane_runtime, workspace_name, workspace_root,
};
pub use switching::record_started;
pub use targets::{focused_pane, pane_id_in, workspace_id};
