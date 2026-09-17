//! The line typed into a spawned agent's shell to start Claude Code.

use crate::features::agent::logic::launch_command;

#[test]
fn a_spawned_agent_does_not_take_over_the_owners_browser() {
    // With Claude in Chrome on by default, every new session connects to the
    // extension, which brings claude.ai up in the owner's browser. A lead that
    // spawns three agents did that three times.
    let line = launch_command("auto", false, "Begin.");
    assert_eq!(
        line,
        "claude --permission-mode auto --no-chrome \"Begin.\"\r"
    );
}

#[test]
fn an_owner_who_wants_their_agents_in_the_browser_can_have_that() {
    // Says nothing either way, so Claude Code's own setting decides - and an
    // older Claude Code that has no such flag is not handed one.
    let line = launch_command("acceptEdits", true, "Begin.");
    assert_eq!(line, "claude --permission-mode acceptEdits \"Begin.\"\r");
}
