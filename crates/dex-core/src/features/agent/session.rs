//! How Claude Code's sessions map onto agents, kept pure. One Claude Code in
//! one pane is one agent, whatever its session id does: `/clear` and an
//! in-session `/resume` end one session and start another - SessionEnd, then
//! SessionStart with another id - and the agent, its id, its children and its
//! messages carry on through it (issue #55).

/// How long after its agent ended a `clear` or `compact` start is still that
/// agent carrying on, when its SessionEnd arrived first and ended it.
pub const CARRY_ON_MS: i64 = 60_000;

/// How far a start's stamp may seem to come before its end's: hooks are stamped
/// by separate processes, and the one that ran second may have stamped first.
const SKEW_MS: i64 = 2_000;

/// Whether a SessionEnd is the agent ending. `/clear` and `/resume` end the
/// session, not the agent: a SessionStart follows at once, in the same process.
pub fn ends_the_agent(end_reason: Option<&str>) -> bool {
    !matches!(end_reason, Some("clear") | Some("resume"))
}

/// Whether a SessionStart with this `source` is the pane's live agent carrying
/// on under a new session id, rather than a session beginning.
pub fn keeps_live_agent(source: Option<&str>) -> bool {
    matches!(source, Some("clear") | Some("compact") | Some("resume"))
}

/// Whether a SessionStart with this `source` may bring back the pane's agent
/// that ended moments ago. Not `resume`: `claude --resume` in a fresh process
/// is found by its session id, and could be any earlier conversation.
pub fn revives_just_ended(source: Option<&str>) -> bool {
    matches!(source, Some("clear") | Some("compact"))
}

/// Whether a start stamped `stamp` is the agent whose ending hook was stamped
/// `ended`: moments later - both stamps from hooks, so on one clock. Long
/// after, whoever is in the pane is someone new.
pub fn carries_on(ended: i64, stamp: i64) -> bool {
    stamp + SKEW_MS >= ended && stamp - ended <= CARRY_ON_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_quitting_ends_the_agent() {
        assert!(!ends_the_agent(Some("clear")));
        assert!(!ends_the_agent(Some("resume")));
        for reason in [
            Some("prompt_input_exit"),
            Some("logout"),
            Some("other"),
            None,
        ] {
            assert!(ends_the_agent(reason), "{reason:?}");
        }
    }

    #[test]
    fn clear_compact_and_resume_keep_the_live_agent_startup_does_not() {
        for source in ["clear", "compact", "resume"] {
            assert!(keeps_live_agent(Some(source)), "{source}");
        }
        assert!(!keeps_live_agent(Some("startup")));
        assert!(!keeps_live_agent(None));
    }

    #[test]
    fn only_clear_and_compact_bring_back_an_agent_that_just_ended() {
        assert!(revives_just_ended(Some("clear")));
        assert!(revives_just_ended(Some("compact")));
        assert!(!revives_just_ended(Some("resume")));
        assert!(!revives_just_ended(Some("startup")));
    }

    #[test]
    fn carrying_on_is_moments_after_the_end_and_no_longer() {
        assert!(carries_on(1_000, 1_000));
        assert!(carries_on(1_000, 1_000 + CARRY_ON_MS));
        assert!(!carries_on(1_000, 1_001 + CARRY_ON_MS));
        // Stamped by another process a moment early: still the same agent.
        assert!(carries_on(5_000, 3_500));
        assert!(!carries_on(10_000, 1_000));
    }
}
