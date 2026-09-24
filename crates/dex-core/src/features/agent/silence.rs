//! When an agent has gone quiet, and what Dex does about it.
//!
//! Two rules, both pure, that read the same thing - how long since the agent's
//! last hook - and answer different questions. The watchdog acts on one; the
//! other is only ever said out loud.

use dex_protocol::agent::AgentStatus;

/// The watchdog's verdict: a running agent with no hook event and no pane
/// output for `quiet_ms` has gone silent and is shown as `unknown`.
pub fn is_silent(
    status: AgentStatus,
    last_event_at: i64,
    last_output_at: Option<i64>,
    now: i64,
    quiet_ms: i64,
) -> bool {
    status == AgentStatus::Running
        && now - last_event_at >= quiet_ms
        && now - last_output_at.unwrap_or(0) >= quiet_ms
}

/// How long a running agent has gone without a hook, when that is long enough
/// to be worth saying (issue #67). Hooks fire at every prompt and every tool
/// batch, so a gap means the agent has not finished a tool call in all that
/// time: a long build, or a command that will never come back.
///
/// Only said, never acted on. From here the two look alike, and the one thing
/// worse than a stuck agent is Dex ending a working one. It is the watchdog's
/// `is_silent` that acts, and it cannot see this: Claude Code redraws its own
/// screen while it waits, so the pane is never quiet even when the agent is.
pub fn quiet_for(status: AgentStatus, last_event_at: i64, now: i64, after_ms: i64) -> Option<i64> {
    let quiet = now - last_event_at;
    (status == AgentStatus::Running && quiet >= after_ms).then_some(quiet)
}

#[cfg(test)]
mod tests {
    use super::{is_silent, quiet_for};
    use dex_protocol::agent::AgentStatus;

    const AFTER: i64 = 5 * 60_000;

    #[test]
    fn a_running_agent_that_has_not_hooked_for_a_while_is_worth_saying_so() {
        assert_eq!(
            quiet_for(AgentStatus::Running, 0, AFTER, AFTER),
            Some(AFTER)
        );
        assert_eq!(
            quiet_for(AgentStatus::Running, 1_000, AFTER + 61_000, AFTER),
            Some(AFTER + 60_000)
        );
    }
    #[test]
    fn a_tool_call_shorter_than_the_threshold_is_just_work() {
        assert_eq!(quiet_for(AgentStatus::Running, 0, AFTER - 1, AFTER), None);
    }
    #[test]
    fn only_a_running_agent_is_quiet_rather_than_finished_or_waiting() {
        // Idle has ended its turn; waiting is waiting on the owner, who knows
        // it; unknown is the watchdog already saying this louder.
        for status in [
            AgentStatus::Idle,
            AgentStatus::Waiting,
            AgentStatus::Error,
            AgentStatus::Unknown,
            AgentStatus::Dead,
        ] {
            assert_eq!(quiet_for(status, 0, AFTER * 10, AFTER), None, "{status:?}");
        }
    }
    #[test]
    fn a_hook_stamped_ahead_of_the_clock_is_not_a_long_silence() {
        assert_eq!(
            quiet_for(AgentStatus::Running, AFTER * 2, AFTER, AFTER),
            None
        );
    }
    #[test]
    fn the_watchdog_flags_only_silent_running_agents() {
        let quiet = 120_000;
        assert!(is_silent(AgentStatus::Running, 0, None, quiet, quiet));
        assert!(
            !is_silent(AgentStatus::Running, 0, Some(quiet - 1), quiet, quiet),
            "recent output"
        );
        assert!(
            !is_silent(AgentStatus::Running, 1, None, quiet, quiet),
            "recent hook"
        );
        assert!(
            !is_silent(AgentStatus::Waiting, 0, None, quiet, quiet),
            "waiting on a human is fine"
        );
    }
}
