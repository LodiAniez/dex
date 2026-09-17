//! Command tests against a real `AppState` over a temp database, including
//! the two mandatory regressions (conventions §5): subagent hook events leave
//! the pane's status unchanged, and out-of-order events resolve to the newest.

use dex_protocol::agent::{
    AgentEventArgs, AgentStatus, AgentView, ListAgentsArgs, PaneExitedArgs, StopAgentArgs,
};
use dex_protocol::workspace::CreateWorkspaceArgs;
use serde_json::{Value, json};

use super::{AgentError, event, list, pane_exited, stop, store, sweep};
use crate::app::AppState;
use crate::features::workspace::{self, WorkspaceError};

mod launch;
mod orphans;
mod spawn;
mod stopping;

/// A state with one workspace; returns its first pane's id.
async fn pane() -> (tempfile::TempDir, AppState, String) {
    let (dir, state) = AppState::for_tests();
    let list = workspace::create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let pane = list.workspaces[0].panes[0].id.clone();
    (dir, state, pane)
}

fn hook(kind: &str, pane: &str, stamp: i64, input: Value) -> AgentEventArgs {
    AgentEventArgs {
        kind: kind.into(),
        pane: pane.into(),
        agent: None,
        stamp,
        input,
    }
}

fn session(id: &str) -> Value {
    json!({ "session_id": id, "permission_mode": "default" })
}

async fn fire(state: &AppState, kind: &str, pane: &str, stamp: i64, input: Value) {
    event(state, hook(kind, pane, stamp, input)).await.unwrap();
}

async fn agents(state: &AppState) -> Vec<AgentView> {
    list(
        state,
        ListAgentsArgs {
            workspace: None,
            pane: None,
            include_dead: true,
        },
    )
    .await
    .unwrap()
    .agents
}

async fn only_agent(state: &AppState) -> AgentView {
    let all = agents(state).await;
    assert_eq!(all.len(), 1, "{all:?}");
    all.into_iter().next().unwrap()
}

#[tokio::test]
async fn a_session_goes_idle_running_waiting_running_idle_dead() {
    let (_dir, state, pane) = pane().await;
    let start = json!({ "session_id": "s1", "source": "startup" });
    fire(&state, "session-start", &pane, 1, start).await;
    assert_eq!(only_agent(&state).await.status, AgentStatus::Idle);

    let steps = [
        ("prompt", AgentStatus::Running),
        ("permission", AgentStatus::Waiting),
        ("batch", AgentStatus::Running),
        ("stop", AgentStatus::Idle),
        ("session-end", AgentStatus::Dead),
    ];
    for (i, (kind, expected)) in steps.into_iter().enumerate() {
        fire(&state, kind, &pane, 10 + i as i64, session("s1")).await;
        assert_eq!(only_agent(&state).await.status, expected, "after {kind}");
    }
    assert!(only_agent(&state).await.ended_at.is_some());
}

#[tokio::test]
async fn subagent_hook_events_leave_the_pane_status_unchanged() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1", "source": "startup" }),
    )
    .await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;

    // A subagent finishing carries the parent's session id plus its own agent_id.
    let from_subagent = json!({ "session_id": "s1", "agent_id": "sub-1", "agent_type": "Explore" });
    let outcome = event(&state, hook("stop", &pane, 3, from_subagent))
        .await
        .unwrap();

    assert!(!outcome.applied);
    assert_eq!(only_agent(&state).await.status, AgentStatus::Running);
}

#[tokio::test]
async fn out_of_order_status_events_resolve_to_the_newest() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1", "source": "startup" }),
    )
    .await;

    // Stamped in the order prompt(20) < stop(30) < prompt(40), but delivered shuffled.
    fire(&state, "prompt", &pane, 40, session("s1")).await;
    fire(&state, "stop", &pane, 30, session("s1")).await;
    fire(&state, "prompt", &pane, 20, session("s1")).await;
    assert_eq!(only_agent(&state).await.status, AgentStatus::Running);

    fire(&state, "stop", &pane, 50, session("s1")).await;
    assert_eq!(only_agent(&state).await.status, AgentStatus::Idle);
}

#[tokio::test]
async fn clear_keeps_the_same_agent_and_status_under_the_new_session_id() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1", "source": "startup" }),
    )
    .await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    let before = only_agent(&state).await;

    fire(
        &state,
        "session-start",
        &pane,
        3,
        json!({ "session_id": "s2", "source": "clear" }),
    )
    .await;
    fire(&state, "stop", &pane, 4, session("s2")).await;

    let after = only_agent(&state).await;
    assert_eq!(after.id, before.id);
    assert_eq!(after.status, AgentStatus::Idle);
}

#[tokio::test]
async fn a_new_session_in_the_same_pane_ends_the_previous_agent() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1", "source": "startup" }),
    )
    .await;
    fire(
        &state,
        "session-start",
        &pane,
        2,
        json!({ "session_id": "s2", "source": "startup" }),
    )
    .await;

    let all = agents(&state).await;
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].status, AgentStatus::Idle, "the newest is live");
    assert_eq!(all[1].status, AgentStatus::Dead, "the older one ended");
}

#[tokio::test]
async fn hooks_from_a_session_dex_never_saw_start_register_it() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "batch", &pane, 5, session("s9")).await;
    assert_eq!(only_agent(&state).await.status, AgentStatus::Running);
}

#[tokio::test]
async fn stop_failure_records_the_failure_type() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "prompt", &pane, 1, session("s1")).await;
    fire(
        &state,
        "stop-failure",
        &pane,
        2,
        json!({ "session_id": "s1", "error": "rate_limit" }),
    )
    .await;
    let agent = only_agent(&state).await;
    assert_eq!(agent.status, AgentStatus::Error);
    assert_eq!(agent.status_detail.as_deref(), Some("rate_limit"));
}

#[tokio::test]
async fn unknown_kinds_are_ignored_and_unknown_panes_are_errors() {
    let (_dir, state, pane) = pane().await;
    let ignored = event(&state, hook("teleport", &pane, 1, session("s1")))
        .await
        .unwrap();
    assert!(!ignored.applied);
    assert!(agents(&state).await.is_empty());
    assert!(
        event(&state, hook("prompt", "no-such-pane", 1, session("s1")))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_panes_process_exiting_ends_its_agent() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "prompt", &pane, 1, session("s1")).await;
    let outcome = pane_exited(&state, PaneExitedArgs { pane: pane.clone() })
        .await
        .unwrap();
    assert!(outcome.applied);
    assert_eq!(only_agent(&state).await.status, AgentStatus::Dead);
    let live = list(&state, ListAgentsArgs::default()).await.unwrap();
    assert!(
        live.agents.is_empty(),
        "ended agents are hidden unless asked for"
    );
}

#[tokio::test]
async fn the_watchdog_marks_a_long_silent_running_agent_unknown() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "prompt", &pane, 1, session("s1")).await;
    assert!(
        !sweep(&state).await.unwrap().applied,
        "a fresh agent is not silent"
    );

    let id = only_agent(&state).await.id;
    state
        .db
        .call(move |conn| store::touch(conn, &id, None, 0))
        .await
        .unwrap();
    assert!(sweep(&state).await.unwrap().applied);
    assert_eq!(only_agent(&state).await.status, AgentStatus::Unknown);

    // Any later hook brings it back.
    fire(
        &state,
        "batch",
        &pane,
        crate::platform::clock::now_millis(),
        session("s1"),
    )
    .await;
    assert_eq!(only_agent(&state).await.status, AgentStatus::Running);
}

#[tokio::test]
async fn list_takes_a_workspace_name_and_rejects_unknown_ones() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "prompt", &pane, 1, session("s1")).await;
    let name = workspace::list(&state).await.unwrap().workspaces[0]
        .name
        .clone();

    let scoped = ListAgentsArgs {
        workspace: Some(name),
        pane: None,
        include_dead: false,
    };
    assert_eq!(list(&state, scoped).await.unwrap().agents.len(), 1);

    let unknown = ListAgentsArgs {
        workspace: Some("nope".into()),
        pane: None,
        include_dead: false,
    };
    assert!(matches!(
        list(&state, unknown).await,
        Err(AgentError::Target(WorkspaceError::NoSuchWorkspace(_)))
    ));
}

fn stop_args(target: &str) -> StopAgentArgs {
    StopAgentArgs {
        agent: target.into(),
    }
}

#[tokio::test]
async fn stop_finds_the_agent_by_its_id_or_its_pane() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "prompt", &pane, 1, session("s1")).await;
    let id = only_agent(&state).await.id;

    // Test panes have no shell, so there is nothing to interrupt; stopping is
    // then ending the row, which proves the target resolved. A second stop,
    // by the other name, finds the same agent already ended.
    let stopped = stop(&state, stop_args(&pane)).await.unwrap();
    assert_eq!(stopped.agent, id);
    let again = stop(&state, stop_args(&id)).await;
    assert!(
        matches!(again, Err(AgentError::NotRunning(ref ended)) if ended == &id),
        "{again:?}"
    );
}

#[tokio::test]
async fn a_hook_after_an_agent_was_ended_revives_the_same_agent() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "prompt", &pane, 1, session("s1")).await;
    let ended = only_agent(&state).await.id;

    // What `agent.stop` does: end the row without waiting for a SessionEnd
    // that a killed Claude Code never sends.
    let id = ended.clone();
    state
        .db
        .call(move |conn| store::end_agent(conn, &id, 10))
        .await
        .unwrap();
    assert_eq!(only_agent(&state).await.status, AgentStatus::Dead);

    // Claude Code survived the interrupt and keeps hooking, same session id.
    fire(&state, "batch", &pane, 20, session("s1")).await;

    // One agent, not two: identity is (pane_id, session_id) (PRD §9.1), so a
    // second row would break the unique index as well as the model.
    let agent = only_agent(&state).await;
    assert_eq!(agent.id, ended);
    assert_eq!(agent.status, AgentStatus::Running);
    assert_eq!(agent.ended_at, None, "it is no longer over");
}

#[tokio::test]
async fn a_late_hook_from_before_the_end_leaves_the_agent_dead() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "prompt", &pane, 30, session("s1")).await;
    let id = only_agent(&state).await.id;
    let ending = id.clone();
    state
        .db
        .call(move |conn| store::end_agent(conn, &ending, 40))
        .await
        .unwrap();

    // An async hook stamped before the end, delivered after it.
    fire(&state, "batch", &pane, 35, session("s1")).await;

    let agent = only_agent(&state).await;
    assert_eq!(agent.id, id, "no second agent was invented");
    assert_eq!(agent.status, AgentStatus::Dead);
}

#[tokio::test]
async fn stop_refuses_unknown_targets_and_ended_agents() {
    let (_dir, state, pane) = pane().await;
    assert!(matches!(
        stop(&state, stop_args(&pane)).await,
        Err(AgentError::NoSuchAgent(_))
    ));
    assert!(matches!(
        stop(&state, stop_args("nothing-by-this-name")).await,
        Err(AgentError::NoSuchAgent(_))
    ));

    fire(&state, "prompt", &pane, 1, session("s1")).await;
    fire(&state, "session-end", &pane, 2, session("s1")).await;
    let id = only_agent(&state).await.id;
    assert!(matches!(
        stop(&state, stop_args(&id)).await,
        Err(AgentError::NotRunning(_))
    ));
}
