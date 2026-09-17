//! `agent.stop` and the pane an agent leaves behind. Dex made a spawned
//! agent's pane, so Dex removes it; a pane the owner opened is the owner's.

use dex_protocol::agent::{AgentStatus, SpawnArgs};

use super::{agents, fire, pane, session, stop_args};
use crate::features::agent::{spawn, stop};
use crate::features::workspace;

fn brief(task: &str, pane: &str) -> SpawnArgs {
    SpawnArgs {
        task: task.into(),
        repo: None,
        worktree: None,
        label: None,
        direction: None,
        pane: Some(pane.into()),
        workspace: None,
    }
}

async fn pane_ids(state: &crate::app::AppState) -> Vec<String> {
    workspace::list(state).await.unwrap().workspaces[0]
        .panes
        .iter()
        .map(|pane| pane.id.clone())
        .collect()
}

#[tokio::test]
async fn stopping_a_spawned_agent_takes_its_pane_with_it() {
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("lead")).await;
    let child = spawn(&state, brief("count to ten", &first)).await.unwrap();
    assert!(pane_ids(&state).await.contains(&child.pane));

    let stopped = stop(&state, stop_args(&child.agent)).await.unwrap();

    assert!(stopped.closed_pane, "{stopped:?}");
    assert_eq!(
        pane_ids(&state).await,
        [first],
        "only the lead's pane is left"
    );
    let row = agents_all(&state)
        .await
        .into_iter()
        .find(|a| a.id == child.agent)
        .unwrap();
    assert_eq!(row.status, AgentStatus::Dead);
}

#[tokio::test]
async fn stopping_an_agent_the_owner_started_leaves_their_pane_alone() {
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("mine")).await;
    let id = agents(&state).await[0].id.clone();

    let stopped = stop(&state, stop_args(&id)).await.unwrap();

    assert!(!stopped.closed_pane, "{stopped:?}");
    assert_eq!(pane_ids(&state).await, [first]);
}

#[tokio::test]
async fn a_spawned_agent_in_the_workspaces_last_pane_is_ended_and_the_pane_kept() {
    // A workspace keeps at least one pane; refusing the stop over that would
    // leave an agent nobody can end.
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("lead")).await;
    let child = spawn(&state, brief("count to ten", &first)).await.unwrap();
    workspace::close_pane(&state, dex_protocol::workspace::PaneArgs { pane: first })
        .await
        .unwrap();

    let stopped = stop(&state, stop_args(&child.agent)).await.unwrap();

    assert!(!stopped.closed_pane);
    assert_eq!(pane_ids(&state).await, [child.pane]);
}

async fn agents_all(state: &crate::app::AppState) -> Vec<dex_protocol::agent::AgentView> {
    crate::features::agent::list(
        state,
        dex_protocol::agent::ListAgentsArgs {
            include_dead: true,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .agents
}

#[test]
fn an_idle_agent_is_simply_told_to_exit() {
    use crate::features::agent::logic::{ExitStep, exit_plan};
    assert_eq!(exit_plan(AgentStatus::Idle), [ExitStep::Type("/exit")]);
}

#[test]
fn a_busy_agent_is_interrupted_first_or_the_command_would_queue_as_a_message() {
    use crate::features::agent::logic::{ExitStep, exit_plan};
    // Text typed during a turn is queued for the model; text typed at a
    // permission dialog answers the dialog. Escape clears both.
    for status in [
        AgentStatus::Running,
        AgentStatus::Waiting,
        AgentStatus::Error,
        AgentStatus::Unknown,
    ] {
        assert_eq!(
            exit_plan(status),
            [ExitStep::Escape, ExitStep::Type("/exit")],
            "{status:?}"
        );
    }
}

#[tokio::test]
async fn clocking_out_closes_the_pane_even_of_an_agent_the_owner_started() {
    // Clock-out is the owner's own, confirmed, choice about that pane.
    let (_dir, state, first) = pane().await;
    let second = workspace::split_pane(
        &state,
        dex_protocol::workspace::SplitPaneArgs {
            pane: first.clone(),
            direction: dex_protocol::workspace::SplitDirection::Right,
            cwd: None,
            label: None,
            kind: None,
        },
    )
    .await
    .unwrap()
    .workspaces[0]
        .active_pane
        .clone()
        .unwrap();
    fire(&state, "session-start", &second, 1, session("mine")).await;
    let id = agents(&state).await[0].id.clone();

    let stopped = stop(
        &state,
        dex_protocol::agent::StopAgentArgs {
            agent: id,
            graceful: true,
            close_pane: true,
        },
    )
    .await
    .unwrap();

    assert!(stopped.closed_pane, "{stopped:?}");
    assert_eq!(pane_ids(&state).await, [first]);
}
