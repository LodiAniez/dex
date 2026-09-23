//! What Dex does with a setting it cannot use.
//!
//! Never refuses to start over one: the value goes back to its default and the
//! owner is told which and why, because a GUI that will not open is worse than
//! a setting that did not take. What each bound is for is written where it is
//! enforced.

use super::{Config, PERMISSION_MODES, REMOVED_VIEW, VIEWS};

/// Below this, an ordinary tool call would be called quiet and the line would
/// mean nothing.
const MIN_QUIET_SECONDS: u64 = 60;

/// Above this - a day - the line could never appear, and a number large enough
/// to overflow the millis it is turned into would turn it inside out.
const MAX_QUIET_SECONDS: u64 = 24 * 60 * 60;

/// The smallest digest budget worth building; below this a digest cannot say
/// anything useful and the agent would be better off with none.
const MIN_DIGEST: usize = 200;

/// Every value the owner cannot have, put back to its default, with a line
/// each saying what was wrong. Called on load and on every hot reload.
pub(super) fn everything_unusable(config: &mut Config) -> Vec<String> {
    let mut problems = Vec::new();
    let defaults = Config::default();

    // Watermarks that cross would pause the PTY and never resume it.
    if config.flow.low_bytes >= config.flow.high_bytes || config.flow.high_bytes == 0 {
        problems.push(format!(
            "flow.low_bytes ({}) must be below flow.high_bytes ({}); using {} and {}",
            config.flow.low_bytes,
            config.flow.high_bytes,
            defaults.flow.low_bytes,
            defaults.flow.high_bytes
        ));
        config.flow = defaults.flow;
    }
    if config.agents.max_depth < 1 {
        problems.push(format!(
            "agents.max_depth ({}) must be at least 1; using 1",
            config.agents.max_depth
        ));
        config.agents.max_depth = 1;
    }
    if config.agents.max_concurrent < 1 {
        problems.push(format!(
            "agents.max_concurrent ({}) must be at least 1; using 1",
            config.agents.max_concurrent
        ));
        config.agents.max_concurrent = 1;
    }
    if !(MIN_QUIET_SECONDS..=MAX_QUIET_SECONDS).contains(&config.agents.quiet_after_seconds) {
        problems.push(format!(
            "agents.quiet_after_seconds ({}) must be between {MIN_QUIET_SECONDS} and {MAX_QUIET_SECONDS}; using {}",
            config.agents.quiet_after_seconds, defaults.agents.quiet_after_seconds
        ));
        config.agents.quiet_after_seconds = defaults.agents.quiet_after_seconds;
    }
    if !PERMISSION_MODES.contains(&config.agents.spawn_permission_mode.as_str()) {
        problems.push(format!(
            "agents.spawn_permission_mode \"{}\" is not one of {}; using \"{}\"",
            config.agents.spawn_permission_mode,
            PERMISSION_MODES.join(", "),
            defaults.agents.spawn_permission_mode
        ));
        config.agents.spawn_permission_mode = defaults.agents.spawn_permission_mode.clone();
    }
    if config.digest.full_chars < MIN_DIGEST || config.digest.delta_chars < MIN_DIGEST {
        problems.push(format!(
            "digest budgets must be at least {MIN_DIGEST} characters; using {} and {}",
            defaults.digest.full_chars, defaults.digest.delta_chars
        ));
        config.digest = defaults.digest;
    }
    if config.ui.view == REMOVED_VIEW.0 {
        problems.push(format!(
            "ui.view \"{}\": the cards view was removed; using \"{}\"",
            REMOVED_VIEW.0, REMOVED_VIEW.1
        ));
        config.ui.view = REMOVED_VIEW.1.into();
    } else if !VIEWS.contains(&config.ui.view.as_str()) {
        problems.push(format!(
            "ui.view \"{}\" is not one of {}; using \"{}\"",
            config.ui.view,
            VIEWS.join(", "),
            defaults.ui.view
        ));
        config.ui = defaults.ui;
    }
    problems
}
