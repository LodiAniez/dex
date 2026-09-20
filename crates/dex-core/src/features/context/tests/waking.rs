//! Waking an agent for messages waiting (issue #58): a message that arrives
//! while an agent is working is read when that turn ends, not left for ever.
//!
//! No pane has a shell in tests, so the typing itself fails; what is checked
//! is the decision and its mark - which agent Dex set out to wake, and that it
//! wakes once per message.

use dex_protocol::agent::AgentEventArgs;
use dex_protocol::context::MessageArgs;
use serde_json::json;

use super::{from, second_pane, start_agent, workspace_at};
use crate::app::AppState;
use crate::features::agent;
use crate::features::context::{message_send, store};

async fn hook(state: &AppState, pane: &str, kind: &str, session: &str) {
    agent::event(
        state,
        AgentEventArgs {
            kind: kind.into(),
            pane: pane.into(),
            agent: None,
            stamp: 2,
            input: json!({ "session_id": session }),
        },
    )
    .await
    .unwrap();
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

#[tokio::test]
async fn a_message_to_a_working_agent_waits_and_wakes_it_when_its_turn_ends() {
    let (_root, _dir, state, lead) = workspace_at().await;
    let child = second_pane(&state, &lead).await;
    start_agent(&state, &lead, "s-lead").await;
    start_agent(&state, &child, "s-child").await;
    // The child is working when the message arrives.
    hook(&state, &child, "prompt", "s-child").await;

    let sent = message_send(
        &state,
        MessageArgs {
            target_agent: child.clone(),
            body: "the requirement changed".into(),
            caller: from(&lead),
        },
    )
    .await
    .unwrap();
    assert!(!sent.woken, "it is working: it reads this at its next turn");
    assert_eq!(woken_at(&state, &child).await, 0);

    // Its turn ends: now it is woken for what came in meanwhile.
    hook(&state, &child, "stop", "s-child").await;
    assert_eq!(woken_at(&state, &child).await, sent.seq);
}

#[tokio::test]
async fn an_agent_that_ignored_its_wake_is_not_woken_again_for_the_same_message() {
    let (_root, _dir, state, lead) = workspace_at().await;
    let child = second_pane(&state, &lead).await;
    start_agent(&state, &lead, "s-lead").await;
    start_agent(&state, &child, "s-child").await;

    let first = message_send(
        &state,
        MessageArgs {
            target_agent: child.clone(),
            body: "one".into(),
            caller: from(&lead),
        },
    )
    .await
    .unwrap();
    assert!(first.woken, "idle: woken as the message arrives");

    // Another turn ends with the message still unread: no second wake.
    hook(&state, &child, "stop", "s-child").await;
    assert_eq!(woken_at(&state, &child).await, first.seq);

    // A newer message does wake it again.
    let second = message_send(
        &state,
        MessageArgs {
            target_agent: child.clone(),
            body: "two".into(),
            caller: from(&lead),
        },
    )
    .await
    .unwrap();
    assert!(second.woken);
    assert_eq!(woken_at(&state, &child).await, second.seq);
}

#[tokio::test]
async fn a_message_read_by_the_agent_leaves_nothing_to_wake_it_for() {
    let (_root, _dir, state, lead) = workspace_at().await;
    let child = second_pane(&state, &lead).await;
    start_agent(&state, &lead, "s-lead").await;
    start_agent(&state, &child, "s-child").await;
    hook(&state, &child, "prompt", "s-child").await;
    message_send(
        &state,
        MessageArgs {
            target_agent: child.clone(),
            body: "one".into(),
            caller: from(&lead),
        },
    )
    .await
    .unwrap();

    // It read its inbox during the turn, then the turn ended.
    crate::features::context::inbox(
        &state,
        dex_protocol::context::ScopeArgs::for_caller(from(&child)),
    )
    .await
    .unwrap();
    hook(&state, &child, "stop", "s-child").await;

    assert_eq!(woken_at(&state, &child).await, 0, "nothing was waiting");
}
