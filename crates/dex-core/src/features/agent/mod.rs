//! Agent registry and lifecycle: the Claude Code sessions running in panes.
//! Tables: `agent`.
//! Commands: `agent.event` (hooks), `agent.list`, `agent.stop`, `agent.pane_exited`,
//! `agent.sweep` (watchdog).
//! Which hook means what, and how a status moves, is pure and lives in `logic.rs`.

mod commands;
mod identity;
mod logic;
mod model;
mod store;
#[cfg(test)]
mod tests;

pub use commands::{event, list, pane_exited, stop, sweep};
pub use identity::{brief_of, label_of, resolve_agent, siblings};
pub use model::AgentError;
