//! Cross-agent context: entries, the event log, read cursors, and digests.
//! Tables: `context_entry`, `context_entry_fts`, `context_event`, `context_cursor`.
//! Commands: `context.*` and the MCP tool calls.
//! Digest assembly and budgeting are pure and live in `logic.rs`.

mod commands;
mod logic;
mod model;
mod store;
#[cfg(test)]
mod tests;
