//! Who a message names (issue #59). A label names an agent in the sender's
//! own workspace: labels are unique within one, not across them, so the same
//! `reviewer` may be at work in the workspace next door.

use dex_protocol::context::{MessageArgs, ScopeArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{from, workspace_at};
use crate::app::AppState;
use crate::features::agent;
use crate::features::context::{inbox, message_send};
use crate::features::workspace;

#[tokio::test]
async fn a_message_by_label_goes_to_the_senders_own_workspace() {
    // Labels are unique per workspace, so `reviewer` can be at work in two of
    // them; a message must reach the sender's own (issue #59).
    let (_root, _dir, state, here) = workspace_at().await;
    let mine = labelled_agent(&state, &here, "reviewer").await;

    let other_root = tempfile::tempdir().unwrap();
    let created = workspace::create(
        &state,
        CreateWorkspaceArgs {
            name: Some("other".into()),
            root_path: Some(other_root.path().to_string_lossy().replace('\\', "/")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let there = created
        .workspaces
        .iter()
        .find(|ws| ws.name == "other")
        .unwrap()
        .panes[0]
        .id
        .clone();
    // Started later: the one the old, workspace-blind lookup would have taken.
    let theirs = labelled_agent(&state, &there, "reviewer").await;

    message_send(
        &state,
        MessageArgs {
            target_agent: "reviewer".into(),
            body: "the requirement changed".into(),
            caller: from(&here),
        },
    )
    .await
    .unwrap();

    let ours = inbox(&state, ScopeArgs::for_caller(from(&mine)))
        .await
        .unwrap();
    assert_eq!(ours.messages.len(), 1, "our own reviewer read it");
    let strangers = inbox(&state, ScopeArgs::for_caller(from(&theirs)))
        .await
        .unwrap();
    assert!(
        strangers.messages.is_empty(),
        "the other workspace's did not"
    );
}

/// Spawns an agent labelled `label` beside `pane`, and returns its pane.
async fn labelled_agent(state: &AppState, pane: &str, label: &str) -> String {
    agent::spawn(
        state,
        dex_protocol::agent::SpawnArgs {
            task: format!("be the {label}"),
            repo: None,
            worktree: None,
            label: Some(label.to_owned()),
            direction: None,
            pane: Some(pane.to_owned()),
            workspace: None,
        },
    )
    .await
    .unwrap()
    .pane
}
