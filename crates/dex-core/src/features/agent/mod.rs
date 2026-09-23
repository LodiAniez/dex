//! Agent registry and lifecycle: the Claude Code sessions running in panes.
//! Tables: `agent`.
//! Commands: `agent.event` (hooks), `agent.list`, `agent.stop`, `agent.prompt`, `agent.pane_exited`,
//! `agent.sweep` (the watchdog, in `watchdog.rs`).
//! Which hook means what, and how a status moves, is pure and lives in `logic.rs`.

mod asking;
mod commands;
mod identity;
mod listing;
mod logic;
mod model;
mod placement;
mod presence;
mod prompt;
mod reason;
mod session;
mod silence;
mod spawn;
mod stop;
mod store;
#[cfg(test)]
mod tests;
mod waking;
mod watchdog;

pub use commands::{event, pane_exited};
pub use identity::{
    Whereabouts, brief_of, ended_in_workspace, label_of, resolve_agent, siblings, whereabouts,
};
pub use listing::list;
pub use model::AgentError;
pub use prompt::prompt;
pub use spawn::spawn;
pub use stop::stop;
pub use waking::waits_at_its_prompt;
/// For the context slice's tests: what the sweep does about messages waiting.
#[cfg(test)]
pub use waking::wake_waiting;
pub use watchdog::sweep;
