//! An agent is Claude Code running in a pane. When the process table shows
//! the pane's shell with no Claude Code under it, there is no agent.

use std::collections::HashSet;

use super::{agents, fire, pane, session};
use crate::features::agent::{presence, store};
use crate::platform::proctree::Proc;

fn proc(pid: u32, parent: u32, name: &str) -> Proc {
    Proc {
        pid,
        parent,
        name: name.into(),
        command: String::new(),
    }
}

fn candidate(id: &str, shell: u32) -> (String, u32) {
    (id.into(), shell)
}

#[test]
fn an_agent_whose_shell_has_no_claude_under_it_has_gone() {
    let procs = [
        proc(100, 1, "pwsh.exe"),
        proc(101, 100, "claude.exe"),
        proc(200, 1, "pwsh.exe"),
        proc(201, 200, "git.exe"),
    ];
    let gone = presence::gone(&procs, &[candidate("here", 100), candidate("left", 200)]);
    assert_eq!(gone, HashSet::from(["left".to_owned()]));
}

#[test]
fn an_agent_the_table_cannot_speak_for_is_left_alone() {
    // A shell that is not in the table, and a pane running inside WSL: no evidence, no verdict.
    let procs = [proc(300, 1, "pwsh.exe"), proc(301, 300, "wsl.exe")];
    let gone = presence::gone(
        &procs,
        &[candidate("unlisted", 999), candidate("in-wsl", 300)],
    );
    assert!(gone.is_empty());
}

#[test]
fn only_an_agent_gone_both_times_it_was_looked_for_is_ended() {
    // Claude Code replaces itself when it updates: gone for a moment is not gone.
    let first = HashSet::from(["blinked".to_owned(), "left".to_owned()]);
    let second = HashSet::from(["left".to_owned(), "just-now".to_owned()]);
    assert_eq!(
        presence::confirmed(&first, &second),
        vec!["left".to_owned()]
    );
}

#[tokio::test]
async fn only_a_living_agent_that_has_started_in_a_pane_is_looked_for() {
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("started")).await;
    let id = agents(&state).await[0].id.clone();

    let looked_for = state
        .db
        .call(|conn| store::presence_candidates(conn))
        .await
        .unwrap();
    assert_eq!(looked_for, vec![(id.clone(), first.clone())]);

    state
        .db
        .call(move |conn| store::end_agent(conn, &id, 5))
        .await
        .unwrap();
    assert!(
        state
            .db
            .call(|conn| store::presence_candidates(conn))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_sweep_with_nobody_to_look_for_does_not_read_the_process_table() {
    // No agents: the look is skipped, and says it ended nobody.
    let (_dir, state, _first) = pane().await;
    assert_eq!(presence::end_the_departed(&state).await.unwrap(), 0);
}

async fn pane_ids(state: &crate::app::AppState) -> Vec<String> {
    crate::features::workspace::list(state)
        .await
        .unwrap()
        .workspaces[0]
        .panes
        .iter()
        .map(|pane| pane.id.clone())
        .collect()
}

async fn log_bodies(state: &crate::app::AppState, pane: &str) -> Vec<String> {
    crate::features::context::events(
        state,
        dex_protocol::context::ScopeArgs::for_caller(dex_protocol::context::Caller {
            pane: Some(pane.to_owned()),
            ..Default::default()
        }),
    )
    .await
    .unwrap()
    .events
    .into_iter()
    .map(|event| event.body)
    .collect()
}

#[tokio::test]
async fn whoever_has_departed_is_ended_and_the_workspace_is_told_why() {
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("s1")).await;
    let id = agents(&state).await[0].id.clone();

    let ended = presence::end_confirmed(&state, vec![id.clone()])
        .await
        .unwrap();

    assert_eq!(ended, 1);
    assert_eq!(
        agents(&state).await[0].status,
        dex_protocol::agent::AgentStatus::Dead
    );
    // A lead reads this log: a child that crashed must not just stop answering.
    let log = log_bodies(&state, &first).await;
    assert!(
        log.iter()
            .any(|body| body.contains("is dead")
                && body.contains("Claude Code is no longer running")),
        "{log:?}"
    );
    // The owner opened this pane: it stays, a shell that may say why.
    assert!(pane_ids(&state).await.contains(&first));
}

#[tokio::test]
async fn a_spawned_agent_that_departed_takes_its_pane_with_it() {
    use dex_protocol::agent::SpawnArgs;
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("lead")).await;
    let child = crate::features::agent::spawn(
        &state,
        SpawnArgs {
            task: "count to ten".into(),
            repo: None,
            worktree: None,
            label: None,
            direction: None,
            pane: Some(first.clone()),
            workspace: None,
        },
    )
    .await
    .unwrap();
    assert!(pane_ids(&state).await.contains(&child.pane));

    presence::end_confirmed(&state, vec![child.agent.clone()])
        .await
        .unwrap();

    // Dex made that pane; a lead whose agents crash is not left a row of empty shells.
    assert!(!pane_ids(&state).await.contains(&child.pane));
    assert!(pane_ids(&state).await.contains(&first));
}

#[tokio::test]
async fn someone_who_ended_in_the_meantime_is_not_ended_or_announced_twice() {
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("s1")).await;
    let id = agents(&state).await[0].id.clone();
    fire(&state, "session-end", &first, 2, session("s1")).await;

    assert_eq!(presence::end_confirmed(&state, vec![id]).await.unwrap(), 0);
    let deaths = log_bodies(&state, &first)
        .await
        .iter()
        .filter(|body| body.contains("is dead"))
        .count();
    assert_eq!(deaths, 1);
}
