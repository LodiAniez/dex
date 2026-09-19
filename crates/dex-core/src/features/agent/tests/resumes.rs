//! `/resume`: back to a conversation the pane had, on to one it never saw,
//! in either order of its hooks, and never into another agent's identity
//! (issue #55).

use dex_protocol::agent::AgentStatus;

use super::sessions::{end, start};
use super::{agents, fire, only_agent, pane, session};

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
async fn resuming_a_conversation_this_pane_already_had_brings_back_its_own_agent() {
    // Agent B ran s0 and quit; agent A runs s1; A's owner /resumes s0.
    // Real times: reviving a row that has ended compares with the daemon's clock.
    let (_dir, state, pane) = pane().await;
    let t = crate::platform::clock::now_millis();
    fire(
        &state,
        "session-start",
        &pane,
        t + 1_000,
        start("s0", "startup"),
    )
    .await;
    let b = only_agent(&state).await;
    fire(
        &state,
        "session-end",
        &pane,
        t + 2_000,
        end("s0", "prompt_input_exit"),
    )
    .await;
    fire(
        &state,
        "session-start",
        &pane,
        t + 3_000,
        start("s1", "startup"),
    )
    .await;
    fire(&state, "session-end", &pane, t + 4_000, end("s1", "resume")).await;
    fire(
        &state,
        "session-start",
        &pane,
        t + 5_000,
        start("s0", "resume"),
    )
    .await;
    fire(&state, "prompt", &pane, t + 6_000, session("s0")).await;

    let live: Vec<_> = agents(&state)
        .await
        .into_iter()
        .filter(|agent| agent.status != AgentStatus::Dead)
        .collect();
    assert_eq!(live.len(), 1, "one Claude Code, one live agent: {live:?}");
    assert_eq!(live[0].id, b.id, "s0's own agent is back");
}

#[tokio::test]
async fn resuming_on_from_a_resumed_conversation_keeps_that_agent() {
    // B ran s0 and quit; A (s1) resumed s0, so B is back; B then resumes s9.
    let (_dir, state, pane) = pane().await;
    let t = crate::platform::clock::now_millis();
    fire(&state, "session-start", &pane, t, start("s0", "startup")).await;
    let b = only_agent(&state).await;
    fire(
        &state,
        "session-end",
        &pane,
        t + 1_000,
        end("s0", "prompt_input_exit"),
    )
    .await;
    fire(
        &state,
        "session-start",
        &pane,
        t + 2_000,
        start("s1", "startup"),
    )
    .await;
    fire(&state, "session-end", &pane, t + 3_000, end("s1", "resume")).await;
    fire(
        &state,
        "session-start",
        &pane,
        t + 4_000,
        start("s0", "resume"),
    )
    .await;
    fire(&state, "session-end", &pane, t + 5_000, end("s0", "resume")).await;
    fire(
        &state,
        "session-start",
        &pane,
        t + 6_000,
        start("s9", "resume"),
    )
    .await;

    let live: Vec<_> = agents(&state)
        .await
        .into_iter()
        .filter(|agent| agent.status != AgentStatus::Dead)
        .collect();
    assert_eq!(live.len(), 1, "{live:?}");
    assert_eq!(live[0].id, b.id, "B carries on, not A");
}

#[tokio::test]
async fn a_resume_whose_start_arrives_before_its_end_leaves_one_live_agent() {
    // SessionEnd hooks run in the background: the start can get here first.
    // Nothing tells that start from a new Claude Code started after Ctrl+C
    // (which sends no end), so it is not given the old agent's identity.
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1_000,
        start("s1", "startup"),
    )
    .await;
    fire(&state, "session-start", &pane, 3_000, start("s9", "resume")).await;
    fire(&state, "session-end", &pane, 2_000, end("s1", "resume")).await;

    let live: Vec<_> = agents(&state)
        .await
        .into_iter()
        .filter(|agent| agent.status != AgentStatus::Dead)
        .collect();
    assert_eq!(live.len(), 1, "{live:?}");
}

#[tokio::test]
async fn claude_resumed_after_ctrl_c_does_not_take_the_old_agents_identity() {
    // Ctrl+C sends no SessionEnd: the old agent is still live when the new
    // Claude Code starts.
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1_000,
        start("s1", "startup"),
    )
    .await;
    let old = only_agent(&state).await;
    fire(&state, "session-start", &pane, 2_000, start("s7", "resume")).await;

    let all = agents(&state).await;
    let now = all.iter().find(|agent| agent.status != AgentStatus::Dead);
    assert_ne!(now.map(|agent| agent.id.clone()), Some(old.id));
}

#[tokio::test]
async fn claude_resumed_in_a_new_process_after_exit_is_not_the_agent_that_quit() {
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
    // `claude --resume` into some other conversation, seconds later.
    fire(&state, "session-start", &pane, 3_000, start("s7", "resume")).await;

    assert_eq!(agents(&state).await.len(), 2);
}

#[tokio::test]
async fn a_spawned_agent_carries_on_through_its_own_resume_when_the_hooks_race() {
    use dex_protocol::agent::AgentEventArgs;
    let (_dir, state, pane) = pane().await;
    let hook =
        |kind: &str, stamp: i64, input: serde_json::Value, id: Option<String>| AgentEventArgs {
            kind: kind.into(),
            pane: pane.clone(),
            agent: id,
            stamp,
            input,
        };
    super::super::event(
        &state,
        hook("session-start", 1_000, start("s1", "startup"), None),
    )
    .await
    .unwrap();
    let child = only_agent(&state).await;
    let id = Some(child.id.clone());
    // The end is processed late (its ended_at is the daemon's clock); the
    // start was stamped before that - both from the one Claude Code.
    super::super::event(
        &state,
        hook("session-end", 2_000, end("s1", "resume"), id.clone()),
    )
    .await
    .unwrap();
    super::super::event(
        &state,
        hook("session-start", 2_500, start("s9", "resume"), id),
    )
    .await
    .unwrap();

    let after = only_agent(&state).await;
    assert_eq!(after.id, child.id);
    assert_ne!(after.status, AgentStatus::Dead, "{after:?}");
}
