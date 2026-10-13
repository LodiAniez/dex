//! Typing into a pane does not go through the database (issue #86).
//!
//! It used to, on every character: `resolve_pane` reads every pane row through
//! Dex's single connection, so each keystroke queued behind every hook from
//! every agent and every query the sweep makes. With several agents hooking at
//! each prompt and tool batch, a pane took no input for a minute and then took
//! all of it at once.
//!
//! The test for that is not a timing: it is a pane the supervisor holds and
//! the database has never heard of. If a write to it succeeds, the lookup was
//! not consulted - there is nothing for it to have found.

use std::path::PathBuf;

use dex_protocol::pane::{LabelPaneArgs, SendArgs, SendKeyArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::super::model::WorkspaceError;
use super::super::{send, send_key};
use crate::app::AppState;
use crate::features::workspace;
use crate::platform::pty::SpawnRequest;

/// A live pane that exists only in the supervisor: no workspace, no row, no
/// label - which is not how Dex makes panes, and is exactly what makes this
/// test say something.
fn only_in_the_supervisor(state: &AppState, pane: &str) {
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
                cols: 80,
                rows: 24,
            },
            Box::new(|_| {}),
        )
        .unwrap();
}

#[tokio::test]
async fn typing_reaches_a_pane_the_database_has_never_heard_of() {
    let (_dir, state) = AppState::for_tests();
    let pane = "a-pane-with-no-row";
    only_in_the_supervisor(&state, pane);

    let sent = send(
        &state,
        SendArgs {
            pane: pane.to_owned(),
            text: "echo hello".to_owned(),
            enter: false,
        },
    )
    .await
    .expect("the supervisor holds this pane, so no lookup is needed");

    assert_eq!(sent.pane, pane);
    state.pty.kill(pane).unwrap();
}

#[tokio::test]
async fn a_key_press_takes_the_same_path() {
    let (_dir, state) = AppState::for_tests();
    let pane = "another-pane-with-no-row";
    only_in_the_supervisor(&state, pane);

    let sent = send_key(
        &state,
        SendKeyArgs {
            pane: pane.to_owned(),
            key: dex_protocol::pane::Key::Enter,
        },
    )
    .await
    .expect("send_key writes through the same path");

    assert_eq!(sent.pane, pane);
    state.pty.kill(pane).unwrap();
}

#[tokio::test]
async fn a_pane_named_by_its_label_is_still_looked_up() {
    // What the lookup exists for: `dex pane send --pane reviewer`. The label
    // is in no supervisor map, so only the database can turn it into a pane -
    // and this is the test that fails if the fast path swallows everything.
    let (_dir, state) = AppState::for_tests();
    let list = workspace::create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let pane = list.workspaces[0].panes[0].id.clone();
    workspace::label_pane(
        &state,
        LabelPaneArgs {
            pane: pane.clone(),
            label: Some("reviewer".to_owned()),
        },
    )
    .await
    .unwrap();
    only_in_the_supervisor(&state, &pane);

    let sent = send(
        &state,
        SendArgs {
            pane: "reviewer".to_owned(),
            text: "hello".to_owned(),
            enter: false,
        },
    )
    .await
    .expect("a label reaches its pane through the lookup");

    assert_eq!(sent.pane, pane, "the label resolved to the pane's id");
    state.pty.kill(&pane).unwrap();
}

#[tokio::test]
async fn a_target_that_is_nothing_at_all_says_so_as_a_missing_pane() {
    // And it must be the lookup saying it: without one, the write itself
    // fails and the error is "the pane has not started", which is a different
    // thing and sends the reader somewhere else.
    let (_dir, state) = AppState::for_tests();

    let refused = send(
        &state,
        SendArgs {
            pane: "no-such-pane-anywhere".to_owned(),
            text: "hello".to_owned(),
            enter: false,
        },
    )
    .await;

    assert!(
        matches!(refused, Err(WorkspaceError::NoSuchPane(ref target)) if target == "no-such-pane-anywhere"),
        "the lookup refused it, rather than the write: {refused:?}"
    );
}
