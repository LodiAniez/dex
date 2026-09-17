//! Agent registry and lifecycle: the Claude Code sessions running in panes.
//! Tables: `agent`.
//! Commands: `agent.event` (hooks), `agent.list`, `agent.stop`, `agent.prompt`, `agent.pane_exited`,
//! `agent.sweep` (the watchdog, in `watchdog.rs`).
//! Which hook means what, and how a status moves, is pure and lives in `logic.rs`.

mod asking;
mod commands;
mod identity;
mod logic;
mod model;
mod presence;
mod prompt;
mod reason;
mod spawn;
mod stop;
mod store;
#[cfg(test)]
mod tests;
mod watchdog;

pub use commands::{event, list, pane_exited};
pub use identity::{
    Whereabouts, brief_of, ended_in_workspace, label_of, resolve_agent, siblings, whereabouts,
};
pub use model::AgentError;
pub use prompt::prompt;
pub use spawn::spawn;
pub use stop::stop;
pub use watchdog::sweep;
