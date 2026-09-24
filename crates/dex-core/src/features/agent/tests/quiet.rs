//! What a working agent that has gone quiet is told to look like (issue #67).
//!
//! Hooks fire at every tool batch, so a working agent that has sent none for
//! a while is inside one long tool call - a build, or something that will
//! never come back. Dex says so and leaves it alone: from here the two look
//! alike, and ending a working agent would be worse than saying nothing.

use dex_protocol::agent::AgentStatus;
use dex_protocol::workspace::CreateWorkspaceArgs;
use serde_json::json;

use super::{fire, only_agent, session, store, sweep};
use crate::app::AppState;
use crate::features::workspace;

/// A state whose agents are called quiet the moment they are working, and one
/// with the setting as it ships; each with a pane of its own.
async fn working(quiet_after_seconds: Option<u64>) -> (tempfile::TempDir, AppState, String) {
    let (dir, state) = AppState::for_tests_with(|config| {
        if let Some(seconds) = quiet_after_seconds {
            config.agents.quiet_after_seconds = seconds;
        }
    });
    let list = workspace::create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let pane = list.workspaces[0].panes[0].id.clone();
    fire(
        &state,
        "session-start",
        &pane,
        1,
        serde_json::json!({ "session_id": "s1", "source": "startup" }),
    )
    .await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    (dir, state, pane)
}

#[tokio::test]
async fn a_working_agent_past_the_threshold_says_how_long_it_has_been_quiet() {
    let (_dir, state, _pane) = working(Some(0)).await;

    let agent = only_agent(&state).await;

    assert_eq!(agent.status, AgentStatus::Running);
    // Counted from the hook a moment ago, not from nothing: measuring the
    // silence against the wrong clock would read as zero for ever (review).
    let quiet = agent.quiet_for_ms.expect("a silence to report");
    assert!((0..5_000).contains(&quiet), "{quiet}ms since its last hook");
    assert!(
        agent.last_event_at > 0,
        "and the view says when that hook was, so a client can go on counting"
    );
}

#[tokio::test]
async fn an_agent_that_has_just_hooked_says_nothing_about_being_quiet() {
    // The setting as it ships: five minutes, which a hook a moment ago is
    // nowhere near.
    let (_dir, state, _pane) = working(None).await;

    assert_eq!(only_agent(&state).await.quiet_for_ms, None);
}

#[tokio::test]
async fn an_agent_that_has_finished_its_turn_is_not_quiet_but_idle() {
    let (_dir, state, pane) = working(Some(0)).await;
    fire(&state, "stop", &pane, 3, session("s1")).await;

    let agent = only_agent(&state).await;

    assert_eq!(agent.status, AgentStatus::Idle);
    assert_eq!(agent.quiet_for_ms, None, "idle is not a silence to report");
}

#[tokio::test]
async fn the_sweep_tells_the_app_to_look_again_while_an_agent_is_quiet() {
    // Crossing the threshold changes nothing in the database, so without this
    // the app would go on showing the listing it already had, and the line
    // would never appear where the owner is looking (review).
    let (_dir, state, _pane) = working(Some(0)).await;
    let mut changes = state.bus.subscribe();

    sweep(&state).await.unwrap();

    let told = changes.try_recv().expect("the app is told to re-read");
    assert_eq!(told.topic, "agents");
}

#[tokio::test]
async fn a_busy_agent_that_has_just_hooked_makes_no_noise_on_the_bus() {
    let (_dir, state, _pane) = working(None).await;
    let mut changes = state.bus.subscribe();

    sweep(&state).await.unwrap();

    assert!(
        changes.try_recv().is_err(),
        "nothing to say, so nothing said"
    );
}

#[tokio::test]
async fn a_subagents_hook_keeps_its_parent_from_looking_quiet() {
    // The parent is inside one `Task` call while its subagent works, so its
    // own hooks stop - but Dex hears the subagent's, and they carry the
    // parent's session (review of issue #67).
    let (_dir, state, pane) = working(None).await;
    let id = only_agent(&state).await.id;
    let long_ago = id.clone();
    state
        .db
        .call(move |conn| store::touch(conn, &long_ago, None, 1))
        .await
        .unwrap();
    assert!(
        only_agent(&state).await.quiet_for_ms.is_some(),
        "quiet, until the subagent is heard from"
    );

    fire(
        &state,
        "stop",
        &pane,
        3,
        json!({ "session_id": "s1", "agent_id": "sub-1", "agent_type": "Explore" }),
    )
    .await;

    let agent = only_agent(&state).await;
    assert_eq!(
        agent.status,
        AgentStatus::Running,
        "a subagent finishing moves no status"
    );
    assert_eq!(agent.quiet_for_ms, None, "but its parent was heard from");
}
