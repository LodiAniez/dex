//! `dex event <kind>`: the Claude Code hook entry point.
//!
//! Always exits 0 and never prints to stdout unless it has a digest to
//! deliver: a failing or noisy hook interrupts the agent's actual work
//! (PRD §9.3). Wired to the daemon in M5.

/// Handles one hook event. A no-op until the agent registry lands (M5).
pub fn run(_kind: &str) {}
