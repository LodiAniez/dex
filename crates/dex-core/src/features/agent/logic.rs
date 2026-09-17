//! Pure functions for the agent slice: which hook means what, and how an
//! agent's status moves (docs/prd.md §9.2). No I/O, no database, no clock.

use dex_protocol::agent::AgentStatus;
use serde_json::Value;

/// A hook firing, as `dex event <kind>` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookKind {
    SessionStart,
    Prompt,
    Batch,
    Permission,
    Waiting,
    Idle,
    Stop,
    StopFailure,
    SessionEnd,
}

impl HookKind {
    /// The kind named by `dex event <kind>`. Unknown names are `None`, and the
    /// daemon ignores them: a newer hook set must not break an older daemon.
    pub fn parse(kind: &str) -> Option<Self> {
        Some(match kind {
            "session-start" => Self::SessionStart,
            "prompt" => Self::Prompt,
            "batch" => Self::Batch,
            "permission" => Self::Permission,
            "waiting" => Self::Waiting,
            "idle" => Self::Idle,
            "stop" => Self::Stop,
            "stop-failure" => Self::StopFailure,
            "session-end" => Self::SessionEnd,
            _ => return None,
        })
    }
}

/// What the daemon needs from a hook's stdin JSON. Field names are the ones
/// captured from Claude Code 2.1.272 (ARCHITECTURE.md); unknown fields are
/// ignored and missing ones default, so a changed payload degrades gently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookInput {
    pub session_id: Option<String>,
    /// SessionStart: `startup`, `resume`, `clear`, `compact`, or `fork`.
    pub source: Option<String>,
    pub permission_mode: Option<String>,
    /// StopFailure: the failure type, e.g. `rate_limit`.
    pub failure: Option<String>,
    /// Set when the hook fired inside one of Claude Code's own subagents.
    pub from_subagent: bool,
}

/// Picks the fields the daemon uses out of a hook's input.
pub fn read_input(input: &Value) -> HookInput {
    let text = |key: &str| {
        input
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    HookInput {
        session_id: text("session_id"),
        source: text("source"),
        permission_mode: text("permission_mode"),
        // Not yet captured from a real StopFailure; these are the likely names.
        failure: ["error", "error_type", "reason"].into_iter().find_map(text),
        // Hooks also fire inside Claude Code's subagents, carrying the parent's
        // session_id plus an agent_id (PRD §9.1).
        from_subagent: input.get("agent_id").is_some_and(|id| !id.is_null()),
    }
}

/// How a SessionStart relates to the agent already in the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStart {
    /// `startup`, `resume`, `fork`: a session begins; register or rebind an agent.
    Begins,
    /// `clear`, `compact`: the same agent carries on, maybe under a new session id.
    Continues,
}

/// Classifies a SessionStart by its `source`.
pub fn session_start(source: Option<&str>) -> SessionStart {
    match source {
        Some("clear") | Some("compact") => SessionStart::Continues,
        _ => SessionStart::Begins,
    }
}

/// The status a hook moves an agent to (PRD §9.2).
pub fn status_after(kind: HookKind) -> AgentStatus {
    match kind {
        HookKind::SessionStart | HookKind::Idle | HookKind::Stop => AgentStatus::Idle,
        HookKind::Prompt | HookKind::Batch => AgentStatus::Running,
        HookKind::Permission | HookKind::Waiting => AgentStatus::Waiting,
        HookKind::StopFailure => AgentStatus::Error,
        HookKind::SessionEnd => AgentStatus::Dead,
    }
}

/// The status an agent moves to when `kind` arrives with `stamp`, or `None`
/// if nothing changes. Background hooks reach the daemon in any order, so an
/// event older than the current status is dropped: the newest wins. A dead
/// agent stays dead.
pub fn next_status(
    current: AgentStatus,
    status_at: i64,
    kind: HookKind,
    stamp: i64,
) -> Option<AgentStatus> {
    if current == AgentStatus::Dead || stamp < status_at {
        return None;
    }
    let next = status_after(kind);
    (next != current).then_some(next)
}

/// Whether a hook shows that an agent Dex had written off is alive after all.
///
/// A dead agent normally stays dead, but Claude Code does not always get to run
/// its `SessionEnd` hook — Ctrl+C kills it first, and print mode never fires one
/// (ARCHITECTURE.md) — so `agent.stop` ends the row itself and can be wrong. A
/// hook stamped *after* the row ended is proof the session outlived the verdict.
/// A hook stamped before it is just a late delivery, and changes nothing.
pub fn revived(status: AgentStatus, ended_at: Option<i64>, stamp: i64) -> bool {
    status == AgentStatus::Dead && ended_at.is_some_and(|ended| stamp > ended)
}

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

/// The stored name of a status.
pub fn status_name(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Idle => "idle",
        AgentStatus::Running => "running",
        AgentStatus::Waiting => "waiting",
        AgentStatus::Error => "error",
        AgentStatus::Unknown => "unknown",
        AgentStatus::Dead => "dead",
    }
}

/// A status from its stored name; anything unexpected reads as `unknown`.
pub fn status_from_name(name: &str) -> AgentStatus {
    match name {
        "idle" => AgentStatus::Idle,
        "running" => AgentStatus::Running,
        "waiting" => AgentStatus::Waiting,
        "error" => AgentStatus::Error,
        "dead" => AgentStatus::Dead,
        _ => AgentStatus::Unknown,
    }
}

/// The line typed into a spawned agent's shell to start Claude Code.
///
/// `--no-chrome` unless the owner asked otherwise: with Claude in Chrome on by
/// default, every new session connects to the browser extension, which brings
/// claude.ai up in the owner's browser - once per agent spawned. With `chrome`
/// the flag is left out rather than turned on, so Claude Code's own setting
/// decides, and an older Claude Code that has no such flag is not handed one.
pub fn launch_command(permission_mode: &str, chrome: bool, kickoff: &str) -> String {
    let browser = if chrome { "" } else { " --no-chrome" };
    format!("claude --permission-mode {permission_mode}{browser} \"{kickoff}\"\r")
}

/// One thing done to an agent's terminal to make Claude Code leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStep {
    /// Press Escape: interrupts a turn, or dismisses a dialog.
    Escape,
    /// Type this and press Enter.
    Type(&'static str),
}

/// How to ask an agent in this state to leave. At an idle prompt `/exit` is a
/// command. Anywhere else it is not: typed during a turn it is queued for the
/// model as a message, and typed at a permission dialog it answers the dialog.
/// Escape clears both, and leaves the prompt the command needs.
pub fn exit_plan(status: AgentStatus) -> Vec<ExitStep> {
    match status {
        AgentStatus::Idle => vec![ExitStep::Type("/exit")],
        _ => vec![ExitStep::Escape, ExitStep::Type("/exit")],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_installed_hook_kind_parses_and_strangers_do_not() {
        for kind in [
            "session-start",
            "prompt",
            "batch",
            "permission",
            "waiting",
            "idle",
            "stop",
            "stop-failure",
            "session-end",
        ] {
            assert!(HookKind::parse(kind).is_some(), "{kind}");
        }
        assert_eq!(HookKind::parse("teleport"), None);
    }

    #[test]
    fn input_fields_are_read_from_a_captured_payload() {
        let input = read_input(&json!({
            "session_id": "s1", "hook_event_name": "SessionStart", "source": "startup",
            "cwd": "C:\\x", "transcript_path": "t"
        }));
        assert_eq!(input.session_id.as_deref(), Some("s1"));
        assert_eq!(input.source.as_deref(), Some("startup"));
        assert!(!input.from_subagent);
        assert_eq!(read_input(&Value::Null), HookInput::default());
    }

    #[test]
    fn subagent_hooks_are_recognized_by_agent_id() {
        assert!(read_input(&json!({ "session_id": "s1", "agent_id": "sub-1" })).from_subagent);
        assert!(!read_input(&json!({ "session_id": "s1", "agent_id": null })).from_subagent);
    }

    #[test]
    fn stop_failure_type_comes_from_the_first_known_field() {
        assert_eq!(
            read_input(&json!({ "error": "rate_limit" }))
                .failure
                .as_deref(),
            Some("rate_limit")
        );
        assert_eq!(
            read_input(&json!({ "error_type": "overloaded" }))
                .failure
                .as_deref(),
            Some("overloaded")
        );
    }

    #[test]
    fn clear_and_compact_continue_everything_else_begins() {
        assert_eq!(session_start(Some("clear")), SessionStart::Continues);
        assert_eq!(session_start(Some("compact")), SessionStart::Continues);
        assert_eq!(session_start(Some("startup")), SessionStart::Begins);
        assert_eq!(session_start(Some("resume")), SessionStart::Begins);
        assert_eq!(session_start(None), SessionStart::Begins);
    }

    #[test]
    fn transitions_follow_the_prd_table() {
        use AgentStatus::*;
        assert_eq!(next_status(Idle, 0, HookKind::Prompt, 1), Some(Running));
        assert_eq!(
            next_status(Running, 0, HookKind::Permission, 1),
            Some(Waiting)
        );
        assert_eq!(next_status(Waiting, 0, HookKind::Batch, 1), Some(Running));
        assert_eq!(next_status(Running, 0, HookKind::Stop, 1), Some(Idle));
        assert_eq!(
            next_status(Running, 0, HookKind::StopFailure, 1),
            Some(Error)
        );
        assert_eq!(next_status(Unknown, 0, HookKind::Batch, 1), Some(Running));
        assert_eq!(next_status(Idle, 0, HookKind::SessionEnd, 1), Some(Dead));
    }

    #[test]
    fn repeating_a_state_is_a_no_op() {
        assert_eq!(
            next_status(AgentStatus::Running, 0, HookKind::Batch, 5),
            None
        );
    }

    #[test]
    fn an_event_older_than_the_current_status_is_dropped() {
        // Stop stamped at 10 lands after a prompt stamped at 20: still running.
        assert_eq!(
            next_status(AgentStatus::Running, 20, HookKind::Stop, 10),
            None
        );
        assert_eq!(
            next_status(AgentStatus::Running, 20, HookKind::Stop, 20),
            Some(AgentStatus::Idle)
        );
    }

    #[test]
    fn dead_agents_stay_dead() {
        assert_eq!(
            next_status(AgentStatus::Dead, 0, HookKind::Prompt, 99),
            None
        );
    }

    #[test]
    fn a_hook_after_the_end_revives_only_then() {
        use AgentStatus::*;
        // `agent.stop` ended the row at 100; Claude Code survived and hooks on.
        assert!(revived(Dead, Some(100), 101));
        // A hook from before the end is a late delivery, not a sign of life.
        assert!(!revived(Dead, Some(100), 100));
        assert!(!revived(Dead, Some(100), 99));
        // Only the dead are revived, and only when Dex recorded an end.
        assert!(!revived(Running, Some(100), 101));
        assert!(!revived(Dead, None, 101));
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

    #[test]
    fn statuses_round_trip_through_their_stored_names() {
        use AgentStatus::*;
        for status in [Idle, Running, Waiting, Error, Unknown, Dead] {
            assert_eq!(status_from_name(status_name(status)), status);
        }
        assert_eq!(status_from_name("garbled"), Unknown);
    }
}
