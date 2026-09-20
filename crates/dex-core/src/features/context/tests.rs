//! Command tests against a real `AppState` over a temp database, including the
//! three `expected_version` cases the PRD calls out (§10.1, §14 M6) and the
//! disk mirror staying in step with the database.

use std::fs;

use dex_protocol::context::{Caller, ListArgs, ReadArgs, SearchArgs, WriteArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::model::ContextError;
use super::{list, read, search, write};
use crate::app::AppState;
use crate::features::workspace;

mod digest;
mod log;
mod messages;
mod waking;

/// A state with one workspace rooted in a temp directory, and its pane id.
async fn workspace_at() -> (tempfile::TempDir, tempfile::TempDir, AppState, String) {
    let root = tempfile::tempdir().unwrap();
    let (dir, state) = AppState::for_tests();
    let args = CreateWorkspaceArgs {
        root_path: Some(root.path().to_string_lossy().replace('\\', "/")),
        ..Default::default()
    };
    let list = workspace::create(&state, args).await.unwrap();
    let pane = list.workspaces[0].panes[0].id.clone();
    (root, dir, state, pane)
}

fn from(pane: &str) -> Caller {
    Caller {
        pane: Some(pane.into()),
        ..Default::default()
    }
}

/// Splits the workspace and returns the new pane's id.
async fn second_pane(state: &AppState, pane: &str) -> String {
    let list = workspace::split_pane(
        state,
        dex_protocol::workspace::SplitPaneArgs {
            pane: pane.into(),
            direction: dex_protocol::workspace::SplitDirection::Right,
            cwd: None,
            label: None,
            kind: None,
        },
    )
    .await
    .unwrap();
    list.workspaces[0]
        .panes
        .iter()
        .map(|p| p.id.clone())
        .find(|id| id != pane)
        .expect("the split added a pane")
}

/// Registers a real agent in a pane, as a `SessionStart` hook would, and
/// returns its id.
async fn start_agent(state: &AppState, pane: &str, session: &str) -> String {
    crate::features::agent::event(
        state,
        dex_protocol::agent::AgentEventArgs {
            kind: "session-start".into(),
            pane: pane.into(),
            agent: None,
            stamp: 1,
            input: serde_json::json!({ "session_id": session, "source": "startup" }),
        },
    )
    .await
    .unwrap();
    crate::features::agent::list(state, dex_protocol::agent::ListAgentsArgs::default())
        .await
        .unwrap()
        .agents
        .into_iter()
        .find(|agent| agent.pane_id.as_deref() == Some(pane))
        .expect("the agent was registered")
        .id
}

fn write_args(key: &str, value: &str, expected: Option<i64>, caller: Caller) -> WriteArgs {
    WriteArgs {
        key: key.into(),
        value: value.into(),
        tags: None,
        expected_version: expected,
        caller,
    }
}

#[tokio::test]
async fn a_written_entry_reads_back_with_its_version() {
    let (_root, _dir, state, pane) = workspace_at().await;
    let written = write(
        &state,
        write_args("auth/jwt", "15 minutes", None, from(&pane)),
    )
    .await
    .unwrap();
    assert_eq!(written.version, 1);

    let entry = read(
        &state,
        ReadArgs {
            key: "auth/jwt".into(),
            caller: from(&pane),
        },
    )
    .await
    .unwrap();
    assert_eq!(entry.value, "15 minutes");
    assert_eq!(entry.version, 1);
}

#[tokio::test]
async fn writing_again_without_a_version_wins_and_bumps() {
    let (_root, _dir, state, pane) = workspace_at().await;
    write(&state, write_args("k", "first", None, from(&pane)))
        .await
        .unwrap();
    let second = write(&state, write_args("k", "second", None, from(&pane)))
        .await
        .unwrap();
    assert_eq!(second.version, 2, "last write wins and bumps the version");
}

#[tokio::test]
async fn two_agents_writing_one_key_with_versions_conflict() {
    let (_root, _dir, state, pane) = workspace_at().await;
    write(&state, write_args("shared", "a", None, from(&pane)))
        .await
        .unwrap();

    // Both read version 1, then both write against it. The second must lose.
    write(&state, write_args("shared", "b", Some(1), from(&pane)))
        .await
        .unwrap();
    let loser = write(&state, write_args("shared", "c", Some(1), from(&pane))).await;

    match loser {
        Err(ContextError::VersionConflict {
            expected,
            current,
            value,
            ..
        }) => {
            assert_eq!((expected, current), (1, 2));
            assert_eq!(value, "b", "the conflict carries what is actually stored");
        }
        other => panic!("expected a version conflict, got {other:?}"),
    }
    // The losing write did not land.
    let entry = read(
        &state,
        ReadArgs {
            key: "shared".into(),
            caller: from(&pane),
        },
    )
    .await
    .unwrap();
    assert_eq!(entry.value, "b");
}

#[tokio::test]
async fn expected_version_zero_means_create_only() {
    let (_root, _dir, state, pane) = workspace_at().await;
    let created = write(&state, write_args("new", "value", Some(0), from(&pane)))
        .await
        .unwrap();
    assert_eq!(created.version, 1);

    let again = write(&state, write_args("new", "other", Some(0), from(&pane))).await;
    assert!(
        matches!(again, Err(ContextError::VersionConflict { current: 1, .. })),
        "creating an existing key is a conflict: {again:?}"
    );
}

#[tokio::test]
async fn keys_that_could_escape_the_mirror_are_refused() {
    let (_root, _dir, state, pane) = workspace_at().await;
    for key in ["../escape", "UPPER", "a//b", "con", ""] {
        let result = write(&state, write_args(key, "v", None, from(&pane))).await;
        assert!(
            matches!(result, Err(ContextError::InvalidKey { .. })),
            "{key:?} should be refused, got {result:?}"
        );
    }
}

#[tokio::test]
async fn the_disk_mirror_matches_the_database_after_a_burst() {
    let (root, _dir, state, pane) = workspace_at().await;
    for i in 0..20 {
        write(
            &state,
            write_args(&format!("burst/k{i}"), &format!("v{i}"), None, from(&pane)),
        )
        .await
        .unwrap();
    }
    for i in 0..20 {
        let path = root.path().join(format!(".dex/context/burst/k{i}.md"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("v{i}"),
            "{path:?}"
        );
    }
    let log = fs::read_to_string(root.path().join(".dex/activity.log")).unwrap();
    assert_eq!(log.lines().count(), 20, "one log line per write");
}

#[tokio::test]
async fn search_finds_entries_by_their_value() {
    let (_root, _dir, state, pane) = workspace_at().await;
    write(
        &state,
        write_args(
            "auth/jwt",
            "tokens expire after fifteen minutes",
            None,
            from(&pane),
        ),
    )
    .await
    .unwrap();
    write(&state, write_args("db/pool", "postgres", None, from(&pane)))
        .await
        .unwrap();

    let hits = search(
        &state,
        SearchArgs {
            query: "fifteen".into(),
            limit: None,
            caller: from(&pane),
        },
    )
    .await
    .unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert_eq!(hits.hits[0].key, "auth/jwt");
}

#[tokio::test]
async fn a_query_fts_cannot_parse_returns_nothing_rather_than_failing() {
    let (_root, _dir, state, pane) = workspace_at().await;
    write(&state, write_args("k", "value", None, from(&pane)))
        .await
        .unwrap();
    // A model can easily emit either of these; neither should break the tool.
    for query in ["\"", "AND"] {
        let hits = search(
            &state,
            SearchArgs {
                query: query.into(),
                limit: None,
                caller: from(&pane),
            },
        )
        .await
        .unwrap();
        assert!(hits.hits.is_empty(), "{query:?} returned hits");
    }
}

#[tokio::test]
async fn listing_gives_metadata_and_filters_by_tag() {
    let (_root, _dir, state, pane) = workspace_at().await;
    let mut tagged = write_args("a", "v", None, from(&pane));
    tagged.tags = Some(" api , auth ".into());
    write(&state, tagged).await.unwrap();
    write(&state, write_args("b", "v", None, from(&pane)))
        .await
        .unwrap();

    let all = list(&state, ListArgs::all(from(&pane))).await.unwrap();
    assert_eq!(all.entries.len(), 2);

    let filtered = list(
        &state,
        ListArgs {
            tag: Some("AUTH".into()),
            caller: from(&pane),
        },
    )
    .await
    .unwrap();
    assert_eq!(filtered.entries.len(), 1);
    assert_eq!(filtered.entries[0].key, "a");
    assert_eq!(filtered.entries[0].tags.as_deref(), Some("api,auth"));
}
