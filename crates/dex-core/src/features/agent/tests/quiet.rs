//! What a working agent that has gone quiet is told to look like (issue #67).
//!
//! Hooks fire at every tool batch, so a working agent that has sent none for
//! a while is inside one long tool call - a build, or something that will
//! never come back. Dex says so and leaves it alone: from here the two look
//! alike, and ending a working agent would be worse than saying nothing.

use dex_protocol::agent::AgentStatus;
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{fire, only_agent, session};
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
    assert!(agent.quiet_for_ms.is_some(), "{agent:?}");
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
