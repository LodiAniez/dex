//! Waking an agent for messages waiting (issue #58): a message that arrives
//! while an agent is working is read once that turn ends, not left for ever.
//!
//! The waking itself is the watchdog's (`agent::wake_waiting`), a beat after
//! the turn ends: hooks Claude Code waits on are no place to type into its
//! pane. No pane has a shell in tests, so the typing fails; what is checked is
//! the decision and its mark - whom Dex set out to wake, and how often.

use dex_protocol::agent::AgentEventArgs;
use dex_protocol::context::MessageArgs;
use serde_json::json;

use super::{from, second_pane, start_agent, workspace_at};
use crate::app::AppState;
use crate::features::agent;
use crate::features::context::{message_send, store};

async fn hook(state: &AppState, pane: &str, kind: &str, stamp: i64, input: serde_json::Value) {
    agent::event(
        state,
        AgentEventArgs {
            kind: kind.into(),
            pane: pane.into(),
            agent: None,
            stamp,
            input,
        },
    )
    .await
    .unwrap();
}

async fn turn(state: &AppState, pane: &str, kind: &str, stamp: i64) {
    hook(state, pane, kind, stamp, json!({ "session_id": "s-child" })).await;
}

async fn send(state: &AppState, from_pane: &str, to: &str, body: &str) -> i64 {
    message_send(
        state,
        MessageArgs {
            target_agent: to.to_owned(),
            body: body.to_owned(),
            caller: from(from_pane),
        },
    )
    .await
    .unwrap()
    .seq
}

/// What the daemon recorded as the newest message it woke `pane`'s agent for.
async fn woken_at(state: &AppState, pane: &str) -> i64 {
    let listed = agent::list(
        state,
        dex_protocol::agent::ListAgentsArgs {
            workspace: None,
            pane: Some(pane.to_owned()),
            include_dead: true,
        },
    )
    .await
    .unwrap();
    let id = listed.agents[0].id.clone();
    state
        .db
        .call(move |conn| store::woken_at(conn, &id))
        .await
        .unwrap()
}

/// A workspace with a lead and a child, both with Claude Code started.
async fn pair() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    AppState,
    String,
    String,
) {
    let (root, dir, state, lead) = workspace_at().await;
    let child = second_pane(&state, &lead).await;
    start_agent(&state, &lead, "s-lead").await;
    start_agent(&state, &child, "s-child").await;
    (root, dir, state, lead, child)
}

#[tokio::test]
async fn a_message_to_a_working_agent_waits_and_the_watchdog_wakes_it_after() {
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await; // working
    let seq = send(&state, &lead, &child, "the requirement changed").await;
    assert_eq!(woken_at(&state, &child).await, 0, "it is working");

    // Its turn ends. The hook itself types nothing: Claude Code waits on hooks.
    turn(&state, &child, "stop", 20).await;
    assert_eq!(woken_at(&state, &child).await, 0);

    assert_eq!(
        agent::waking::wake_waiting(&state).await.unwrap(),
        vec![child.clone()]
    );
    assert_eq!(woken_at(&state, &child).await, seq);
}

#[tokio::test]
async fn an_agent_already_idle_with_messages_waiting_is_woken_too() {
    // What every database upgrading from 0.6.3 looks like: idle, unread, and
    // no hook coming.
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    let seq = send(&state, &lead, &child, "one").await;
    turn(&state, &child, "stop", 20).await;

    assert_eq!(agent::waking::wake_waiting(&state).await.unwrap().len(), 1);
    assert_eq!(woken_at(&state, &child).await, seq);
}

#[tokio::test]
async fn an_agent_that_asked_the_owner_something_is_left_to_them() {
    // Its turn ended on a question; typed text would be the owner's answer.
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    send(&state, &lead, &child, "the requirement changed").await;
    hook(
        &state,
        &child,
        "stop",
        20,
        json!({ "session_id": "s-child", "last_assistant_message": "Which database should I use?" }),
    )
    .await;

    assert!(
        agent::waking::wake_waiting(&state)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(woken_at(&state, &child).await, 0);
}

#[tokio::test]
async fn an_agent_is_woken_once_until_it_has_had_a_turn() {
    let (_root, _dir, state, lead, child) = pair().await;
    let first = send(&state, &lead, &child, "one").await;
    assert_eq!(woken_at(&state, &child).await, first, "idle: woken at once");

    // A second message before it has answered the first queues no second prompt.
    let second = send(&state, &lead, &child, "two").await;
    assert_eq!(woken_at(&state, &child).await, first);
    assert!(
        agent::waking::wake_waiting(&state)
            .await
            .unwrap()
            .is_empty()
    );

    // It takes its turn, and still has not read them: woken again for the newer.
    turn(&state, &child, "prompt", 30).await;
    turn(&state, &child, "stop", 40).await;
    assert_eq!(agent::waking::wake_waiting(&state).await.unwrap().len(), 1);
    assert_eq!(woken_at(&state, &child).await, second);
}

#[tokio::test]
async fn a_message_the_agent_read_leaves_nothing_to_wake_it_for() {
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    send(&state, &lead, &child, "one").await;
    crate::features::context::inbox(
        &state,
        dex_protocol::context::ScopeArgs::for_caller(from(&child)),
    )
    .await
    .unwrap();
    turn(&state, &child, "stop", 20).await;

    assert!(
        agent::waking::wake_waiting(&state)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(woken_at(&state, &child).await, 0);
}
