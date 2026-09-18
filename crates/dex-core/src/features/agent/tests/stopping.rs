//! `agent.stop` and the pane an agent leaves behind. Dex made a spawned
//! agent's pane, so Dex removes it; a pane the owner opened is the owner's.

use dex_protocol::agent::{AgentStatus, SpawnArgs, Spawned};

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
        runtime: None,
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
    assert_eq!(
        exit_plan(AgentStatus::Idle, true),
        [ExitStep::Type("/exit")]
    );
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
            exit_plan(status, true),
            [ExitStep::Escape, ExitStep::Type("/exit")],
            "{status:?}"
        );
    }
}

#[test]
fn a_hire_that_has_not_started_is_not_asked_to_leave() {
    use crate::features::agent::logic::exit_plan;
    // Its pane is a bare shell: `/exit` there is a command for the shell.
    // There is nobody to ask, so stopping goes straight to ending it.
    assert!(exit_plan(AgentStatus::Idle, false).is_empty());
    assert!(exit_plan(AgentStatus::Running, false).is_empty());
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
            runtime: None,
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
            from_pane: None,
        },
    )
    .await
    .unwrap();

    assert!(stopped.closed_pane, "{stopped:?}");
    assert_eq!(pane_ids(&state).await, [first]);
}

/// A lead, the child it spawned, and that child's own child, each in its pane.
async fn family(state: &crate::app::AppState, first: &str) -> (String, Spawned, Spawned) {
    fire(state, "session-start", first, 1, session("lead")).await;
    let lead = agents(state).await[0].id.clone();
    let child = spawn(state, brief("first level", first)).await.unwrap();
    fire(state, "session-start", &child.pane, 2, session("child")).await;
    let grandchild = spawn(state, brief("second level", &child.pane))
        .await
        .unwrap();
    (lead, child, grandchild)
}

fn stop_from(pane: &str, target: &str) -> dex_protocol::agent::StopAgentArgs {
    dex_protocol::agent::StopAgentArgs {
        from_pane: Some(pane.into()),
        ..stop_args(target)
    }
}

#[tokio::test]
async fn an_agent_may_stop_the_agents_it_spawned_however_far_down() {
    let (_dir, state, first) = pane().await;
    let (_lead, child, grandchild) = family(&state, &first).await;

    assert!(
        stop(&state, stop_from(&first, &grandchild.agent))
            .await
            .is_ok()
    );
    assert!(stop(&state, stop_from(&first, &child.agent)).await.is_ok());
}

#[tokio::test]
async fn an_agent_may_not_stop_its_parent_or_anyone_elses_agents() {
    // "Stop the other agents", said to a confused child, must not end the
    // lead or the lead's other children - least of all now that stopping
    // closes panes.
    let (_dir, state, first) = pane().await;
    let (lead, child, _grandchild) = family(&state, &first).await;
    let sibling = spawn(&state, brief("another first level", &first))
        .await
        .unwrap();

    for target in [lead.as_str(), sibling.agent.as_str()] {
        let refused = stop(&state, stop_from(&child.pane, target)).await;
        assert!(
            matches!(
                refused,
                Err(crate::features::agent::AgentError::NotYours(_))
            ),
            "{target}: {refused:?}"
        );
    }
    assert_eq!(agents(&state).await.len(), 4, "nobody was ended");
}

#[tokio::test]
async fn an_agent_may_stop_itself_and_the_owner_may_stop_anyone() {
    let (_dir, state, first) = pane().await;
    let (lead, child, grandchild) = family(&state, &first).await;

    // Itself.
    assert!(
        stop(&state, stop_from(&grandchild.pane, &grandchild.agent))
            .await
            .is_ok()
    );
    // The owner: no pane at all (the office), or a pane with no agent in it.
    assert!(stop(&state, stop_args(&child.agent)).await.is_ok());
    assert!(stop(&state, stop_args(&lead)).await.is_ok());
}
