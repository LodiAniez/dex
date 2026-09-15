//! Agent registry, lifecycle state machine, and spawning.
//! Tables: `agent`.
//! Commands: `agent.*`, `event.*`.
//! Status transitions are pure and live in `logic.rs`.

mod commands;
mod logic;
mod model;
mod store;
#[cfg(test)]
mod tests;
