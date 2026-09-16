//! Tests for the append-only half of the store: notes, directed messages,
//! the activity stream, and workspace isolation.

use dex_protocol::context::{Caller, ListArgs, MessageArgs, NoteArgs, ReadArgs, ScopeArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{from, second_pane, start_agent, workspace_at, write_args};
use crate::features::context::model::ContextError;
use crate::features::context::{events, inbox, list, message_send, note, read, write};
use crate::features::workspace;

#[tokio::test]
async fn a_note_lands_in_the_event_log() {
    let (_root, _dir, state, pane) = workspace_at().await;
    note(
        &state,
        NoteArgs {
            body: "started the refactor".into(),
            tags: None,
            caller: from(&pane),
        },
    )
    .await
    .unwrap();

    let log = events(&state, ScopeArgs::for_caller(from(&pane)))
        .await
        .unwrap();
    assert_eq!(log.events.len(), 1);
    assert_eq!(log.events[0].kind, "note");
    assert_eq!(log.events[0].body, "started the refactor");
}

#[tokio::test]
async fn a_message_reaches_its_target_once_and_hides_its_body_from_others() {
    let (_root, _dir, state, pane) = workspace_at().await;
    // Two real agents, one per pane, as two `claude` sessions in a workspace.
    let second = second_pane(&state, &pane).await;
    let sender_id = start_agent(&state, &pane, "s-a").await;
    start_agent(&state, &second, "s-b").await;
    let sender = from(&pane);
    let recipient = from(&second);

    message_send(
        &state,
        MessageArgs {
            // Targeting by pane label is how an agent names a sibling.
            target_agent: second.clone(),
            body: "the schema moved".into(),
            caller: sender,
        },
    )
    .await
    .unwrap();
    assert!(!sender_id.is_empty());

    let first = inbox(&state, ScopeArgs::for_caller(recipient.clone()))
        .await
        .unwrap();
    assert_eq!(first.messages.len(), 1);
    assert_eq!(first.messages[0].body, "the schema moved");

    let second = inbox(&state, ScopeArgs::for_caller(recipient))
        .await
        .unwrap();
    assert!(second.messages.is_empty(), "reading marks them read");

    // The activity log shows that a message happened, never its contents.
    let log = events(&state, ScopeArgs::for_caller(from(&pane)))
        .await
        .unwrap();
    assert_eq!(log.events.len(), 1);
    assert!(
        !log.events[0].body.contains("schema moved"),
        "a directed message's body is for its recipient: {:?}",
        log.events[0].body
    );
}

#[tokio::test]
async fn a_caller_with_no_pane_and_no_workspace_is_refused() {
    let (_root, _dir, state, _pane) = workspace_at().await;
    let result = read(
        &state,
        ReadArgs {
            key: "k".into(),
            caller: Caller::default(),
        },
    )
    .await;
    assert!(
        matches!(result, Err(ContextError::NoWorkspace)),
        "{result:?}"
    );
}

#[tokio::test]
async fn workspaces_do_not_see_each_others_entries() {
    let (_root, _dir, state, pane) = workspace_at().await;
    write(&state, write_args("secret", "one", None, from(&pane)))
        .await
        .unwrap();

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
    let other_pane = created
        .workspaces
        .iter()
        .find(|ws| ws.name == "other")
        .unwrap()
        .panes[0]
        .id
        .clone();

    let entries = list(&state, ListArgs::all(from(&other_pane)))
        .await
        .unwrap();
    assert!(entries.entries.is_empty(), "context is workspace-scoped");
}
