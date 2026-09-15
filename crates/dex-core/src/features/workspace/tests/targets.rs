//! Targets (ids, names, labels) and the CLI-facing pane commands.

use dex_protocol::pane::{CreatePaneArgs, LabelPaneArgs, ListPanesArgs, SendArgs};
use dex_protocol::workspace::{CreateWorkspaceArgs, PaneArgs, WorkspaceArgs, WorkspaceList};

use crate::app::AppState;
use crate::features::workspace::{
    WorkspaceError, close_pane, create, create_pane, label_pane, list, list_panes, send, switch,
};

fn named(name: &str) -> CreateWorkspaceArgs {
    CreateWorkspaceArgs {
        name: Some(name.into()),
        ..CreateWorkspaceArgs::default()
    }
}

fn first_pane(list: &WorkspaceList) -> String {
    list.workspaces[0].panes[0].id.clone()
}

fn labelled(pane: &str, label: &str) -> LabelPaneArgs {
    LabelPaneArgs {
        pane: pane.into(),
        label: Some(label.into()),
    }
}

fn in_workspace(name: &str) -> CreatePaneArgs {
    CreatePaneArgs {
        workspace: Some(name.into()),
        ..CreatePaneArgs::default()
    }
}

#[tokio::test]
async fn workspaces_are_targeted_by_name_case_insensitively() {
    let (_dir, state) = AppState::for_tests();
    create(&state, named("Api")).await.unwrap();
    let list = create(&state, named("web")).await.unwrap();

    let switched = switch(
        &state,
        WorkspaceArgs {
            workspace: "api".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(switched.active.as_ref(), Some(&list.workspaces[0].id));
}

#[tokio::test]
async fn a_name_two_workspaces_share_is_ambiguous_but_ids_still_work() {
    let (_dir, state) = AppState::for_tests();
    create(&state, named("dup")).await.unwrap();
    let list = create(&state, named("dup")).await.unwrap();

    let ambiguous = switch(
        &state,
        WorkspaceArgs {
            workspace: "dup".into(),
        },
    )
    .await;
    assert!(matches!(
        ambiguous,
        Err(WorkspaceError::AmbiguousTarget { candidates, .. }) if candidates.len() == 2
    ));
    let by_id = WorkspaceArgs {
        workspace: list.workspaces[0].id.clone(),
    };
    assert!(switch(&state, by_id).await.is_ok());
}

#[tokio::test]
async fn labels_name_panes_and_are_unique_within_a_workspace() {
    let (_dir, state) = AppState::for_tests();
    let list = create(&state, named("api")).await.unwrap();
    label_pane(&state, labelled(&first_pane(&list), "server"))
        .await
        .unwrap();
    let created = create_pane(&state, in_workspace("api")).await.unwrap();

    assert!(matches!(
        label_pane(&state, labelled(&created.pane, "server")).await,
        Err(WorkspaceError::LabelTaken(_))
    ));
    assert!(matches!(
        label_pane(&state, labelled(&created.pane, "two words")).await,
        Err(WorkspaceError::InvalidLabel)
    ));

    let after = close_pane(
        &state,
        PaneArgs {
            pane: "server".into(),
        },
    )
    .await
    .unwrap();
    let remaining: Vec<String> = after.workspaces[0]
        .panes
        .iter()
        .map(|p| p.id.clone())
        .collect();
    assert_eq!(remaining, [created.pane]);
}

#[tokio::test]
async fn pane_list_scopes_to_one_workspace_and_marks_the_focused_pane() {
    let (_dir, state) = AppState::for_tests();
    create(&state, named("api")).await.unwrap();
    create(&state, named("web")).await.unwrap();
    let created = create_pane(&state, in_workspace("api")).await.unwrap();

    let all = list_panes(&state, ListPanesArgs::default()).await.unwrap();
    assert_eq!(all.panes.len(), 3);

    let api = list_panes(
        &state,
        ListPanesArgs {
            workspace: Some("api".into()),
            pane: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(api.panes.len(), 2);
    assert!(api.panes.iter().all(|p| p.workspace == "api"));
    let focused: Vec<String> = api
        .panes
        .iter()
        .filter(|p| p.focused)
        .map(|p| p.id.clone())
        .collect();
    assert_eq!(focused, std::slice::from_ref(&created.pane));

    let beside = list_panes(
        &state,
        ListPanesArgs {
            workspace: None,
            pane: Some(created.pane),
        },
    )
    .await
    .unwrap();
    assert_eq!(beside.panes.len(), 2);
}

#[tokio::test]
async fn create_pane_uses_the_given_folder_and_rejects_missing_ones() {
    let (dir, state) = AppState::for_tests();
    create(&state, named("api")).await.unwrap();
    let folder = dir.path().to_string_lossy().into_owned();

    let created = create_pane(
        &state,
        CreatePaneArgs {
            cwd: Some(folder.clone()),
            ..CreatePaneArgs::default()
        },
    )
    .await
    .unwrap();
    let panes = list_panes(&state, ListPanesArgs::default()).await.unwrap();
    let new = panes.panes.iter().find(|p| p.id == created.pane).unwrap();
    assert_eq!(new.cwd, folder.replace('\\', "/"));

    let missing = CreatePaneArgs {
        cwd: Some("C:/definitely/not/here".into()),
        ..CreatePaneArgs::default()
    };
    assert!(matches!(
        create_pane(&state, missing).await,
        Err(WorkspaceError::InvalidRoot(_))
    ));
}

#[tokio::test]
async fn sending_to_a_pane_whose_shell_has_not_started_says_so() {
    let (_dir, state) = AppState::for_tests();
    let list = create(&state, named("api")).await.unwrap();
    let args = SendArgs {
        pane: first_pane(&list),
        text: "echo hi".into(),
        enter: true,
    };
    assert!(matches!(
        send(&state, args).await,
        Err(WorkspaceError::PaneNotStarted(_))
    ));
}

#[tokio::test]
async fn revisions_grow_with_every_change_but_not_with_reads() {
    let (_dir, state) = AppState::for_tests();
    let first = create(&state, named("api")).await.unwrap();
    let read = list(&state).await.unwrap();
    assert_eq!(read.revision, first.revision);
    let second = create(&state, named("web")).await.unwrap();
    assert!(second.revision > first.revision);
}
