//! Agents whose pane is gone: the 0.1.2 report of stale rows that counted
//! toward the spawn limit while `stop` refused them.

use dex_protocol::agent::AgentStatus;
use dex_protocol::workspace::{PaneArgs, SplitDirection, SplitPaneArgs};

use super::{agents, fire, pane, session, stop_args};
use crate::app::AppState;
use crate::features::agent::{AgentError, stop, store, sweep};
use crate::features::workspace;

/// An agent in a second pane, and then that pane closed under it: the row is
/// left `idle` with no pane, which is the shape the 0.1.2 bug report described.
async fn orphaned_agent(state: &AppState, first: &str) -> String {
    let list = workspace::split_pane(
        state,
        SplitPaneArgs {
            pane: first.into(),
            direction: SplitDirection::Right,
            cwd: None,
            label: None,
            kind: None,
        },
    )
    .await
    .unwrap();
    let second = list.workspaces[0].active_pane.clone().unwrap();
    fire(state, "session-start", &second, 1, session("orphan")).await;
    let id = agents(state)
        .await
        .into_iter()
        .find(|a| a.pane_id.as_deref() == Some(second.as_str()))
        .unwrap()
        .id;
    workspace::close_pane(state, PaneArgs { pane: second })
        .await
        .unwrap();
    id
}

async fn live_count(state: &AppState, workspace_id: &str) -> i64 {
    let ws = workspace_id.to_owned();
    state
        .db
        .call(move |conn| store::count_live_in_workspace(conn, &ws))
        .await
        .unwrap()
}

#[tokio::test]
async fn an_agent_whose_pane_was_closed_stops_counting_and_is_ended_by_the_sweep() {
    // The 0.1.2 report: three such rows counted toward the six-agent limit
    // while `stop` refused them, so the workspace was stuck below capacity.
    let (_dir, state, first) = pane().await;
    let id = orphaned_agent(&state, &first).await;
    let orphan = agents(&state)
        .await
        .into_iter()
        .find(|a| a.id == id)
        .unwrap();
    assert_eq!(orphan.pane_id, None, "the schema nulls the pane");
    assert_eq!(
        orphan.status,
        AgentStatus::Idle,
        "and says nothing about status"
    );

    // Even before the sweep runs, it must not take a slot.
    assert_eq!(live_count(&state, &orphan.workspace_id).await, 0);

    let outcome = sweep(&state).await.unwrap();
    assert!(outcome.applied, "the sweep reports the change it made");
    let after = agents(&state)
        .await
        .into_iter()
        .find(|a| a.id == id)
        .unwrap();
    assert_eq!(after.status, AgentStatus::Dead);
    assert!(after.ended_at.is_some());
    assert_eq!(after.status_detail.as_deref(), Some("pane closed"));
    assert!(!sweep(&state).await.unwrap().applied, "nothing left to do");
}

#[tokio::test]
async fn stop_ends_an_agent_whose_pane_is_gone_instead_of_refusing() {
    // "already ended" was wrong twice over: the row said idle, and the list
    // the message pointed at still showed it.
    let (_dir, state, first) = pane().await;
    let id = orphaned_agent(&state, &first).await;

    let stopped = stop(&state, stop_args(&id)).await.unwrap();
    assert_eq!(stopped.agent, id);
    assert_eq!(stopped.pane, None, "there was no pane to press Ctrl+C in");
    let after = agents(&state)
        .await
        .into_iter()
        .find(|a| a.id == id)
        .unwrap();
    assert_eq!(after.status, AgentStatus::Dead);
    assert!(after.ended_at.is_some());

    // A second stop is the ended-agent case, as before.
    assert!(matches!(
        stop(&state, stop_args(&id)).await,
        Err(AgentError::NotRunning(_))
    ));
}
