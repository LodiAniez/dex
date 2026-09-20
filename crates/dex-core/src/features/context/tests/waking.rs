//! Waking an agent for messages waiting (issue #58): a message that arrives
//! while an agent is working is read once that turn ends, not left for ever.
//!
//! The waking itself is the watchdog's (`agent::sweep`), a beat after the turn
//! ends: hooks Claude Code waits on are no place to type into its pane.
//!
//! A pane has no shell here unless the test gives it one, so both halves are
//! covered: with a shell the nudge lands and the agent is woken, without one
//! it cannot, and the wake must be given back and the sender told the truth.
//! That the sweep is what calls the waking, after it ends the departed, is
//! checked over the source in `tests/structure.rs` - neither can be seen from
//! here, and neither may be lost.

use std::path::PathBuf;

use dex_protocol::agent::{AgentEventArgs, AgentStatus, ListAgentsArgs};
use dex_protocol::context::{Delivery, MessageArgs, ScopeArgs, Sent};
use serde_json::json;

use super::{from, second_pane, start_agent, workspace_at};
use crate::app::AppState;
use crate::features::agent;
use crate::features::context::{inbox, message_send, take_wake};
use crate::platform::pty::SpawnRequest;

/// Gives the pane a real shell, so a nudge typed into it can land. Without
/// one there is nothing to write to, which is the other half of these tests.
fn with_shell(state: &AppState, pane: &str) {
    let (program, args) = if cfg!(windows) {
        (PathBuf::from("cmd.exe"), Vec::new())
    } else {
        (PathBuf::from("/bin/sh"), Vec::new())
    };
    state
        .pty
        .spawn(
            SpawnRequest {
                pane_id: pane.to_owned(),
                program,
                args,
                cwd: std::env::temp_dir(),
                env: vec![("DEX_PANE_ID".to_owned(), pane.to_owned())],
                cols: 120,
                rows: 30,
            },
            Box::new(|_| {}),
        )
        .unwrap();
}

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

async fn send(state: &AppState, from_pane: &str, to: &str, body: &str) -> Sent {
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
}

/// The agent running in a pane, as clients see it.
async fn agent_in(state: &AppState, pane: &str) -> dex_protocol::agent::AgentView {
    let listed = agent::list(
        state,
        ListAgentsArgs {
            workspace: None,
            pane: Some(pane.to_owned()),
            include_dead: true,
        },
    )
    .await
    .unwrap();
    // The list is the whole workspace's, whichever pane asked.
    listed
        .agents
        .into_iter()
        .find(|agent| agent.pane_id.as_deref() == Some(pane))
        .expect("an agent in that pane")
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
    with_shell(&state, &child);
    turn(&state, &child, "prompt", 10).await; // working
    let sent = send(&state, &lead, &child, "the requirement changed").await;
    assert_eq!(sent.delivery, Delivery::NextTurn);
    // Mid-turn it is left alone: the delta carries the message.
    assert!(agent::wake_waiting(&state).await.unwrap().is_empty());

    // Its turn ends. The hook itself types nothing: Claude Code waits on hooks.
    turn(&state, &child, "stop", 20).await;
    assert_eq!(
        agent::wake_waiting(&state).await.unwrap(),
        vec![child.clone()],
        "the watchdog wakes it once the turn is over"
    );
    state.pty.kill(&child).unwrap();
}

#[tokio::test]
async fn an_agent_at_its_prompt_is_woken_as_the_message_is_sent() {
    // The whole point of the feature, end to end: the nudge is really typed
    // into a real shell, and the sender is told so.
    let (_root, _dir, state, lead, child) = pair().await;
    with_shell(&state, &child);
    turn(&state, &child, "prompt", 10).await;
    turn(&state, &child, "stop", 20).await;

    assert_eq!(
        send(&state, &lead, &child, "the requirement changed")
            .await
            .delivery,
        Delivery::Woken
    );
    // Woken for it already, and it has not answered: no second nudge.
    assert_eq!(
        send(&state, &lead, &child, "and again").await.delivery,
        Delivery::NextTurn
    );
    assert!(agent::wake_waiting(&state).await.unwrap().is_empty());
    state.pty.kill(&child).unwrap();
}

#[tokio::test]
async fn a_nudge_that_never_lands_is_tried_again_next_sweep() {
    // This pane has no shell, so every nudge fails. A wake spent on typing
    // that never happened would leave the message unread for ever.
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    send(&state, &lead, &child, "one").await;
    turn(&state, &child, "stop", 20).await;
    assert!(
        agent::wake_waiting(&state).await.unwrap().is_empty(),
        "nothing was typed, so nobody was woken"
    );

    // The mark is back where it was, so the claim is there to take again.
    let id = agent_in(&state, &child).await.id;
    let again = state
        .db
        .call(move |conn| Ok(take_wake(conn, &id, "p".to_owned(), true, 20)?.is_some()))
        .await
        .unwrap();
    assert!(again, "the wake was given back");
}

#[tokio::test]
async fn the_sender_is_told_the_message_waits_when_the_nudge_cannot_be_typed() {
    // Idle and started, so it is woken - but the pane runs no shell, so the
    // typing fails, and "woken" would be a lie to an agent that may be
    // waiting on the answer.
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    turn(&state, &child, "stop", 20).await;

    assert_eq!(
        send(&state, &lead, &child, "one").await.delivery,
        Delivery::NextTurn
    );
}

#[tokio::test]
async fn a_message_to_an_agent_that_has_ended_says_so() {
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "session-end", 20).await;
    assert_eq!(agent_in(&state, &child).await.status, AgentStatus::Dead);

    let id = agent_in(&state, &child).await.id;
    assert_eq!(
        send(&state, &lead, &id, "one").await.delivery,
        Delivery::Ended
    );
}

#[tokio::test]
async fn an_agent_that_asked_the_owner_something_is_left_to_them() {
    // Its turn ended on a question; typed text would be the owner's answer.
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    hook(
        &state,
        &child,
        "stop",
        20,
        json!({ "session_id": "s-child", "last_assistant_message": "Which database should I use?" }),
    )
    .await;

    assert_eq!(
        send(&state, &lead, &child, "the requirement changed")
            .await
            .delivery,
        Delivery::WaitingOnOwner
    );
    assert!(agent::wake_waiting(&state).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_agent_is_woken_once_until_it_has_had_a_turn() {
    // Against the mark itself: in tests no nudge can land, so this is where
    // "once per message, however many hooks race" is pinned.
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    send(&state, &lead, &child, "one").await;
    turn(&state, &child, "stop", 20).await;
    let idle = agent_in(&state, &child).await;
    let (id, pane) = (idle.id.clone(), child.clone());

    let (first, again) = state
        .db
        .call({
            let (id, pane) = (id.clone(), pane.clone());
            move |conn| {
                let first = take_wake(conn, &id, pane.clone(), true, 20)?.is_some();
                // The hooks that brought it here racing, or a watchdog sweep
                // landing on the same moment: no second nudge.
                let again = take_wake(conn, &id, pane, true, 20)?.is_some();
                Ok((first, again))
            }
        })
        .await
        .unwrap();
    assert_eq!((first, again), (true, false));

    // A second message, and a turn taken without reading either: woken again.
    send(&state, &lead, &child, "two").await;
    let after_a_turn = state
        .db
        .call(move |conn| Ok(take_wake(conn, &id, pane, true, 40)?.is_some()))
        .await
        .unwrap();
    assert!(after_a_turn);
}

#[tokio::test]
async fn a_message_the_agent_read_leaves_nothing_to_wake_it_for() {
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    send(&state, &lead, &child, "one").await;
    inbox(&state, ScopeArgs::for_caller(from(&child)))
        .await
        .unwrap();
    turn(&state, &child, "stop", 20).await;

    assert!(agent::wake_waiting(&state).await.unwrap().is_empty());
}

#[tokio::test]
async fn messages_waiting_show_in_the_recipients_unread_count() {
    let (_root, _dir, state, lead, child) = pair().await;
    turn(&state, &child, "prompt", 10).await;
    send(&state, &lead, &child, "one").await;
    send(&state, &lead, &child, "two").await;
    assert_eq!(agent_in(&state, &child).await.unread, 2);
    assert_eq!(agent_in(&state, &lead).await.unread, 0);

    inbox(&state, ScopeArgs::for_caller(from(&child)))
        .await
        .unwrap();
    assert_eq!(agent_in(&state, &child).await.unread, 0);
}
