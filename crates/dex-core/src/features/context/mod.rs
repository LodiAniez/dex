//! Cross-agent context sharing: the store agents read and write to coordinate.
//! Tables: `context_entry`, `context_entry_fts`, `context_event`, `context_cursor`.
//! Commands: `context.read/write/list/search/note/message_send/inbox/events`.
//! Key rules and mirror paths are pure (`logic.rs`); the disk projection under
//! `<workspace root>/.dex/` is in `mirror.rs`.

mod commands;
mod digest;
mod digest_commands;
mod log_commands;
mod logic;
mod mirror;
mod model;
mod store;
#[cfg(test)]
mod tests;
mod wake;

pub use commands::{list, read, search, write};
pub use digest_commands::digest;
pub use log_commands::{
    clear_events, delete_event, events, inbox, message_send, note, record_event, record_status,
};
pub use model::ContextError;
pub use store::unread_counts;
pub use wake::{nudge, take_wake};
