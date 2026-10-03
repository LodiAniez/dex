//! A silent agent that is inside a tool call is working, not "not responding"
//! (issue #81).
//!
//! Four agents were running test suites with their output redirected to files,
//! as the lead had told them to, so no hook arrived and their panes printed
//! nothing - and all four were branded "not responding" while working. The
//! watchdog now asks whether anything is running under the pane's shell.
//!
//! The rule itself is unit-tested in `agent::silence`, and what it is told
//! comes from `workspace::running_under`, tested there against a real process
//! tree. What these cover is the sweep: that an agent whose shell has nothing
//! under it is still called `unknown` - which is what the status is for - and
//! that `sweep_statuses` takes its own `now`, so no test waits two real
//! minutes for the watchdog's threshold.

use std::path::PathBuf;

use dex_protocol::agent::{AgentEventArgs, AgentStatus};
use serde_json::json;

use super::super::store;
use super::super::watchdog::sweep_statuses;
use super::{only_agent, pane};
use crate::app::AppState;
use crate::platform::clock;
use crate::platform::pty::SpawnRequest;

/// Three minutes on, which is past the watchdog's two.
const LATER: i64 = 180_000;

/// Fires one hook, then backdates it: the agent is `running` and has not been
/// heard from in a long time, which is what the watchdog looks for.
async fn working_since_long_ago(state: &AppState, pane: &str) {
    heard_from(state, pane, "s-busy").await;
    let id = only_agent(state).await.id;
    state
        .db
        .call(move |conn| store::touch(conn, &id, None, 0))
        .await
        .unwrap();
}

async fn heard_from(state: &AppState, pane: &str, session: &str) {
    crate::features::agent::event(
        state,
        AgentEventArgs {
            kind: "prompt".into(),
            pane: pane.into(),
            agent: None,
            stamp: clock::now_millis(),
            input: json!({ "session_id": session }),
        },
    )
    .await
    .unwrap();
}

fn with_a_shell(state: &AppState, pane: &str) {
    let program = if cfg!(windows) {
        PathBuf::from("cmd.exe")
    } else {
        PathBuf::from("/bin/sh")
    };
    state
        .pty
        .spawn(
            SpawnRequest {
                pane_id: pane.to_owned(),
                program,
                args: Vec::new(),
                cwd: std::env::temp_dir(),
                env: vec![("DEX_PANE_ID".to_owned(), pane.to_owned())],
                cols: 120,
                rows: 30,
            },
            Box::new(|_| {}),
        )
        .unwrap();
}

#[tokio::test]
async fn an_agent_whose_shell_sits_at_a_prompt_becomes_unknown() {
    // What the status is for: hooks have stopped arriving and the shell has
    // nothing to show for it.
    let (_dir, state, pane) = pane().await;
    with_a_shell(&state, &pane);
    working_since_long_ago(&state, &pane).await;

    let swept = sweep_statuses(&state, clock::now_millis() + LATER)
        .await
        .unwrap();

    assert_eq!(only_agent(&state).await.status, AgentStatus::Unknown);
    assert!(swept.applied, "and the app is told to look again");
    state.pty.kill(&pane).unwrap();
}

#[tokio::test]
async fn an_agent_whose_pane_has_no_shell_at_all_becomes_unknown() {
    // Nothing can be running under a shell that is not there, which is a
    // different thing from not being able to tell - and must not be swallowed
    // by it, or the watchdog would stop saying anything at all.
    let (_dir, state, pane) = pane().await;
    working_since_long_ago(&state, &pane).await;

    sweep_statuses(&state, clock::now_millis() + LATER)
        .await
        .unwrap();

    assert_eq!(only_agent(&state).await.status, AgentStatus::Unknown);
}

#[tokio::test]
async fn an_agent_heard_from_recently_is_left_alone() {
    let (_dir, state, pane) = pane().await;
    with_a_shell(&state, &pane);
    heard_from(&state, &pane, "s-fresh").await;

    let swept = sweep_statuses(&state, clock::now_millis()).await.unwrap();

    assert_eq!(only_agent(&state).await.status, AgentStatus::Running);
    assert!(!swept.applied, "nothing to say about a fresh agent");
    state.pty.kill(&pane).unwrap();
}
