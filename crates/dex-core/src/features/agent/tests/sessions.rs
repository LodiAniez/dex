//! One Claude Code in one pane is one agent, whatever its session id does.
//! `/clear` and `/resume` end a session and start another (`SessionEnd`, then
//! `SessionStart` with a new id); the agent - its id, its children, its
//! messages - carries on through it (issue #55).

use dex_protocol::agent::AgentStatus;
use serde_json::json;

use super::{agents, fire, only_agent, pane, session};

fn start(id: &str, source: &str) -> serde_json::Value {
    json!({ "session_id": id, "source": source })
}

fn end(id: &str, reason: &str) -> serde_json::Value {
    json!({ "session_id": id, "reason": reason })
}

#[tokio::test]
async fn clear_keeps_the_agent_even_though_claude_code_ends_the_session_first() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, start("s1", "startup")).await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    let before = only_agent(&state).await;

    // What Claude Code sends on /clear, in this order.
    fire(&state, "session-end", &pane, 3, end("s1", "clear")).await;
    fire(&state, "session-start", &pane, 4, start("s2", "clear")).await;
    fire(&state, "stop", &pane, 5, session("s2")).await;

    let after = only_agent(&state).await;
    assert_eq!(after.id, before.id, "the same agent, not a new one");
    assert_eq!(after.status, AgentStatus::Idle);
}

#[tokio::test]
async fn resume_inside_a_session_keeps_the_agent_too() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, start("s1", "startup")).await;
    let before = only_agent(&state).await;

    fire(&state, "session-end", &pane, 2, end("s1", "resume")).await;
    fire(&state, "session-start", &pane, 3, start("s0", "resume")).await;

    let after = only_agent(&state).await;
    assert_eq!(after.id, before.id);
    assert_ne!(after.status, AgentStatus::Dead);
}

#[tokio::test]
async fn a_session_ended_by_clear_is_not_the_agent_ending() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, start("s1", "startup")).await;
    fire(&state, "session-end", &pane, 2, end("s1", "clear")).await;
    assert_ne!(only_agent(&state).await.status, AgentStatus::Dead);
}

#[tokio::test]
async fn a_session_carried_on_moments_after_its_end_brings_the_same_agent_back() {
    // Compacted or cleared, but the end arrived as something other than
    // "clear": the pane's agent that just ended is the one carrying on. Timed
    // by the hooks' own stamps, whatever the daemon's clock says.
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1_000,
        start("s1", "startup"),
    )
    .await;
    let before = only_agent(&state).await;
    fire(&state, "session-end", &pane, 2_000, end("s1", "other")).await;
    fire(
        &state,
        "session-start",
        &pane,
        5_000,
        start("s2", "compact"),
    )
    .await;

    let after = only_agent(&state).await;
    assert_eq!(after.id, before.id);
    assert_ne!(after.status, AgentStatus::Dead);
}

#[tokio::test]
async fn a_long_gone_agent_is_not_brought_back_by_a_later_clear() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1_000,
        start("s1", "startup"),
    )
    .await;
    fire(
        &state,
        "session-end",
        &pane,
        2_000,
        end("s1", "prompt_input_exit"),
    )
    .await;
    // Ten minutes on, whatever is in the pane now is someone new.
    fire(
        &state,
        "session-start",
        &pane,
        602_000,
        start("s2", "clear"),
    )
    .await;

    assert_eq!(agents(&state).await.len(), 2);
}

#[tokio::test]
async fn claude_started_again_right_after_exit_is_a_new_agent() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1_000,
        start("s1", "startup"),
    )
    .await;
    fire(
        &state,
        "session-end",
        &pane,
        2_000,
        end("s1", "prompt_input_exit"),
    )
    .await;
    fire(
        &state,
        "session-start",
        &pane,
        3_000,
        start("s2", "startup"),
    )
    .await;

    let all = agents(&state).await;
    assert_eq!(all.len(), 2, "{all:?}");
}

#[tokio::test]
async fn quitting_still_ends_the_agent_with_or_without_a_reason() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, start("s1", "startup")).await;
    fire(
        &state,
        "session-end",
        &pane,
        2,
        end("s1", "prompt_input_exit"),
    )
    .await;
    assert_eq!(only_agent(&state).await.status, AgentStatus::Dead);

    let (_dir2, state, pane) = super::pane().await;
    fire(&state, "session-start", &pane, 1, start("s1", "startup")).await;
    fire(&state, "session-end", &pane, 2, session("s1")).await;
    assert_eq!(only_agent(&state).await.status, AgentStatus::Dead);
}
