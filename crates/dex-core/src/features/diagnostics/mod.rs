//! `dex doctor` and `dex config`: what is set up, and what Dex is running with.
//! Tables: none, so no `store.rs`.
//! Commands: `config.get`, `config.reload`. (`doctor` runs entirely in the CLI,
//! since most of what it checks is outside the daemon.)
//! Check evaluation is pure and lives in `logic.rs`.

mod commands;
mod logic;
mod model;
#[cfg(test)]
mod tests;

pub use commands::{get_config, reload_config};
pub use model::DiagnosticsError;
