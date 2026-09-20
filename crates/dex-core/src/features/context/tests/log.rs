//! Tests for the append-only half of the store: notes, directed messages,
//! the activity stream, and workspace isolation.

use dex_protocol::context::{Caller, ListArgs, MessageArgs, NoteArgs, ReadArgs, ScopeArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{from, second_pane, start_agent, workspace_at, write_args};
use crate::app::AppState;
use crate::features::agent;
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
async fn an_event_names_the_agent_behind_it_by_id_and_the_human_by_nothing() {
    // A label is for reading and two agents can share one; whoever filters the
    // log down to one agent needs the id.
    let (_root, _dir, state, pane) = workspace_at().await;
    let second = second_pane(&state, &pane).await;
    let say = |pane: &str, body: &str| NoteArgs {
        body: body.into(),
        tags: None,
        caller: from(pane),
    };
    note(&state, say(&pane, "before anyone started"))
        .await
        .unwrap();
    let first_id = start_agent(&state, &pane, "s-a").await;
    let second_id = start_agent(&state, &second, "s-b").await;
    note(&state, say(&pane, "from the first")).await.unwrap();
    note(&state, say(&second, "from the second")).await.unwrap();

    let log = events(&state, ScopeArgs::for_caller(from(&pane)))
        .await
        .unwrap();
    let by = |body: &str| {
        log.events
            .iter()
            .find(|event| event.body == body)
            .unwrap_or_else(|| panic!("no event {body:?} in {:?}", log.events))
            .agent_id
            .clone()
    };
    assert_eq!(by("before anyone started"), None, "the human has no id");
    assert_eq!(by("from the first"), Some(first_id));
    assert_eq!(by("from the second"), Some(second_id));
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
    // Who it was for is not a secret, and a label can be shared: the office
    // walks the sender to the recipient's desk, which needs both by id.
    assert_eq!(log.events[0].agent_id.as_deref(), Some(sender_id.as_str()));
    let recipient_id = crate::features::agent::list(&state, Default::default())
        .await
        .unwrap()
        .agents
        .into_iter()
        .find(|agent| agent.id != sender_id)
        .unwrap()
        .id;
    assert_eq!(log.events[0].target_agent_id, Some(recipient_id));
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

#[tokio::test]
async fn a_message_to_an_idle_agent_is_stored_even_when_its_pane_cannot_be_woken() {
    // The daemon tries to wake an idle recipient by typing a prompt into its
    // pane. In tests no pane has a shell, so that attempt fails — and the
    // message must be stored all the same, waiting for the agent's next turn.
    let (_root, _dir, state, pane) = workspace_at().await;
    let second = second_pane(&state, &pane).await;
    start_agent(&state, &pane, "s-a").await;
    start_agent(&state, &second, "s-b").await;

    let sent = message_send(
        &state,
        MessageArgs {
            target_agent: second.clone(),
            body: "requirement changed: odd shouts hey".into(),
            caller: from(&pane),
        },
    )
    .await
    .expect("a pane with no shell is not a reason to lose the message");
    assert!(sent.seq > 0);

    let delivered = inbox(&state, ScopeArgs::for_caller(from(&second)))
        .await
        .unwrap();
    assert_eq!(delivered.messages.len(), 1);
    assert_eq!(
        delivered.messages[0].body,
        "requirement changed: odd shouts hey"
    );
}

#[tokio::test]
async fn one_event_can_be_removed_and_a_missing_one_says_so() {
    use dex_protocol::context::DeleteEventArgs;

    let (_root, _dir, state, pane) = workspace_at().await;
    for body in ["first", "second"] {
        note(
            &state,
            NoteArgs {
                body: body.into(),
                tags: None,
                caller: from(&pane),
            },
        )
        .await
        .unwrap();
    }
    let before = events(&state, ScopeArgs::for_caller(from(&pane)))
        .await
        .unwrap();
    let first = before.events[0].seq;

    let cleared = crate::features::context::delete_event(
        &state,
        DeleteEventArgs {
            seq: first,
            caller: from(&pane),
        },
    )
    .await
    .unwrap();
    assert_eq!(cleared.removed, 1);
    let after = events(&state, ScopeArgs::for_caller(from(&pane)))
        .await
        .unwrap();
    assert_eq!(after.events.len(), 1);
    assert_eq!(after.events[0].body, "second");

    let again = crate::features::context::delete_event(
        &state,
        DeleteEventArgs {
            seq: first,
            caller: from(&pane),
        },
    )
    .await;
    assert!(
        matches!(again, Err(ContextError::NoSuchEvent(_))),
        "{again:?}"
    );
}

#[tokio::test]
async fn clearing_ended_agents_keeps_the_living_and_the_human() {
    use dex_protocol::agent::AgentEventArgs;
    use dex_protocol::context::{ClearEventsArgs, ClearScope};

    let (_root, _dir, state, pane) = workspace_at().await;
    let second = second_pane(&state, &pane).await;
    let live = start_agent(&state, &pane, "s-live").await;
    let doomed = start_agent(&state, &second, "s-doomed").await;
    assert_ne!(live, doomed);
    for (who, body) in [(&pane, "still here"), (&second, "about to go")] {
        note(
            &state,
            NoteArgs {
                body: body.into(),
                tags: None,
                caller: from(who),
            },
        )
        .await
        .unwrap();
    }
    // The human, from outside any pane: named by workspace, not by pane.
    let workspace_id = {
        let pane = pane.clone();
        state
            .db
            .call(move |conn| workspace::find_pane_workspace(conn, &pane))
            .await
            .unwrap()
            .unwrap()
    };
    note(
        &state,
        NoteArgs {
            body: "typed by a person".into(),
            tags: None,
            caller: Caller {
                pane: None,
                workspace: Some(workspace_id),
                agent: None,
            },
        },
    )
    .await
    .unwrap();
    // The second agent's session ends.
    crate::features::agent::event(
        &state,
        AgentEventArgs {
            kind: "session-end".into(),
            pane: second.clone(),
            agent: None,
            stamp: 99,
            input: serde_json::json!({ "session_id": "s-doomed" }),
        },
    )
    .await
    .unwrap();

    let cleared = crate::features::context::clear_events(
        &state,
        ClearEventsArgs {
            scope: ClearScope::Ended,
            caller: from(&pane),
        },
    )
    .await
    .unwrap();
    assert!(
        cleared.removed >= 1,
        "the ended agent's note (and its status rows) go"
    );
    let left = events(&state, ScopeArgs::for_caller(from(&pane)))
        .await
        .unwrap();
    let bodies: Vec<&str> = left.events.iter().map(|e| e.body.as_str()).collect();
    assert!(bodies.contains(&"still here"), "{bodies:?}");
    assert!(bodies.contains(&"typed by a person"), "{bodies:?}");
    assert!(!bodies.contains(&"about to go"), "{bodies:?}");

    let everything = crate::features::context::clear_events(
        &state,
        ClearEventsArgs {
            scope: ClearScope::All,
            caller: from(&pane),
        },
    )
    .await
    .unwrap();
    assert!(everything.removed >= 2);
    let none = events(&state, ScopeArgs::for_caller(from(&pane)))
        .await
        .unwrap();
    assert!(none.events.is_empty());
}
