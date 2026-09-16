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

pub use commands::{list, read, search, write};
pub use digest_commands::digest;
pub use log_commands::{events, inbox, message_send, note, record_event, record_status};
pub use model::ContextError;
