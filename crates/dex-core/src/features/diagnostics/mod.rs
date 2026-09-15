//! `dex doctor`: environment and installation checks.
//! Tables: none, so no `store.rs`.
//! Commands: `doctor.run`.
//! Check evaluation is pure and lives in `logic.rs`.

mod commands;
mod logic;
mod model;
#[cfg(test)]
mod tests;
