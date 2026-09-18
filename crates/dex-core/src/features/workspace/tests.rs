//! Command tests against a real `AppState` over a temp database. Workspace
//! commands here; pane tree commands in `tests/panes.rs`; targets and the
//! CLI-facing pane commands in `tests/targets.rs`.

use dex_protocol::workspace::{
    CreateWorkspaceArgs, Layout, RecolorWorkspaceArgs, RenameWorkspaceArgs, ReorderWorkspacesArgs,
    WorkspaceArgs, WorkspaceList,
};

use super::{WorkspaceError, create, delete, list, logic, recolor, rename, reorder, store, switch};
use crate::app::AppState;

mod docking;
mod panes;
mod targets;

fn named(name: &str) -> CreateWorkspaceArgs {
    CreateWorkspaceArgs {
        name: Some(name.into()),
        ..CreateWorkspaceArgs::default()
    }
}

fn ids(list: &WorkspaceList) -> Vec<String> {
    list.workspaces.iter().map(|ws| ws.id.clone()).collect()
}

fn names(list: &WorkspaceList) -> Vec<String> {
    list.workspaces.iter().map(|ws| ws.name.clone()).collect()
}

#[tokio::test]
async fn first_workspace_gets_default_name_color_and_one_pane() {
    let (_dir, state) = AppState::for_tests();
    let list = create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();

    let ws = &list.workspaces[0];
    assert_eq!(ws.name, "Workspace 1");
    assert_eq!(ws.color.as_deref(), Some(logic::PALETTE[0]));
    assert_eq!(ws.panes.len(), 1);
    assert_eq!(ws.panes[0].cwd, ws.root_path);
    assert!(
        !ws.root_path.contains('\\'),
        "paths are stored with forward slashes"
    );
    assert_eq!(list.active.as_deref(), Some(ws.id.as_str()));
    assert!(matches!(&ws.layout, Some(Layout::Leaf { pane_id }) if *pane_id == ws.panes[0].id));
}

#[tokio::test]
async fn new_workspaces_append_in_order_take_the_next_color_and_become_active() {
    let (_dir, state) = AppState::for_tests();
    create(&state, named("api")).await.unwrap();
    let list = create(&state, named("web")).await.unwrap();

    assert_eq!(names(&list), ["api", "web"]);
    assert_eq!(list.workspaces[1].color.as_deref(), Some(logic::PALETTE[1]));
    assert_eq!(list.active.as_ref(), Some(&list.workspaces[1].id));
}

#[tokio::test]
async fn rename_rejects_blank_names_and_unknown_workspaces() {
    let (_dir, state) = AppState::for_tests();
    let id = create(&state, named("api")).await.unwrap().workspaces[0]
        .id
        .clone();

    let renamed = rename(
        &state,
        RenameWorkspaceArgs {
            workspace: id.clone(),
            name: " backend ".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(names(&renamed), ["backend"]);

    let blank = rename(
        &state,
        RenameWorkspaceArgs {
            workspace: id,
            name: "  ".into(),
        },
    )
    .await;
    assert!(matches!(blank, Err(WorkspaceError::InvalidName)));
    let missing = rename(
        &state,
        RenameWorkspaceArgs {
            workspace: "nope".into(),
            name: "x".into(),
        },
    )
    .await;
    assert!(matches!(missing, Err(WorkspaceError::NoSuchWorkspace(_))));
}

#[tokio::test]
async fn recolor_normalizes_hex_rejects_junk_and_can_clear() {
    let (_dir, state) = AppState::for_tests();
    let id = create(&state, named("api")).await.unwrap().workspaces[0]
        .id
        .clone();
    let args = |color: Option<&str>| RecolorWorkspaceArgs {
        workspace: id.clone(),
        color: color.map(String::from),
    };

    let list = recolor(&state, args(Some("#ABCDEF"))).await.unwrap();
    assert_eq!(list.workspaces[0].color.as_deref(), Some("#abcdef"));
    assert!(matches!(
        recolor(&state, args(Some("red"))).await,
        Err(WorkspaceError::InvalidColor(_))
    ));
    let cleared = recolor(&state, args(None)).await.unwrap();
    assert_eq!(cleared.workspaces[0].color, None);
}

#[tokio::test]
async fn reorder_applies_a_complete_order_and_rejects_partial_ones() {
    let (_dir, state) = AppState::for_tests();
    for name in ["a", "b", "c"] {
        create(&state, named(name)).await.unwrap();
    }
    let before = list(&state).await.unwrap();
    let mut order = ids(&before);
    order.rotate_left(1);

    let after = reorder(
        &state,
        ReorderWorkspacesArgs {
            order: order.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(names(&after), ["b", "c", "a"]);

    order.pop();
    let partial = reorder(&state, ReorderWorkspacesArgs { order }).await;
    assert!(matches!(partial, Err(WorkspaceError::InvalidOrder)));
}

#[tokio::test]
async fn deleting_the_active_workspace_removes_its_panes_and_activates_the_first() {
    let (_dir, state) = AppState::for_tests();
    create(&state, named("keep")).await.unwrap();
    let list = create(&state, named("doomed")).await.unwrap();
    let doomed = list.workspaces[1].id.clone();

    let after = delete(
        &state,
        WorkspaceArgs {
            workspace: doomed.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(names(&after), ["keep"]);
    assert_eq!(after.active.as_ref(), Some(&after.workspaces[0].id));
    let orphans = state
        .db
        .call(move |conn| store::list_panes(conn, &doomed))
        .await
        .unwrap();
    assert!(
        orphans.is_empty(),
        "panes must cascade with their workspace"
    );
}

#[tokio::test]
async fn switching_to_an_unknown_workspace_is_an_error() {
    let (_dir, state) = AppState::for_tests();
    let result = switch(
        &state,
        WorkspaceArgs {
            workspace: "nope".into(),
        },
    )
    .await;
    assert!(matches!(result, Err(WorkspaceError::NoSuchWorkspace(_))));
}

#[tokio::test]
async fn workspaces_and_the_active_one_survive_reopening_the_database() {
    let (dir, state) = AppState::for_tests();
    create(&state, named("api")).await.unwrap();
    let created = create(&state, named("web")).await.unwrap();
    let api = created.workspaces[0].id.clone();
    switch(
        &state,
        WorkspaceArgs {
            workspace: api.clone(),
        },
    )
    .await
    .unwrap();
    drop(state);

    let (reopened, problems) =
        AppState::open(&dir.path().join("dex.db"), dir.path().join("config.toml")).unwrap();
    assert!(problems.is_empty(), "no config file means no complaints");
    let restored = list(&reopened).await.unwrap();
    assert_eq!(names(&restored), ["api", "web"]);
    assert_eq!(restored.active, Some(api));
    assert_eq!(restored.workspaces[1].panes.len(), 1);
}

#[tokio::test]
async fn a_root_that_is_not_a_directory_is_rejected() {
    let (_dir, state) = AppState::for_tests();
    let args = CreateWorkspaceArgs {
        root_path: Some("C:/definitely/not/here".into()),
        ..CreateWorkspaceArgs::default()
    };
    assert!(matches!(
        create(&state, args).await,
        Err(WorkspaceError::InvalidRoot(_))
    ));
}

mod pane_kinds {
    use dex_protocol::pane::{CreatePaneArgs, ListPanesArgs, PaneContentArgs};

    use super::super::{create_pane, list_panes, pane_content};
    use super::*;

    async fn workspace_in(state: &AppState, dir: &std::path::Path) -> String {
        create(
            state,
            CreateWorkspaceArgs {
                name: Some("kinds".into()),
                root_path: Some(dir.to_string_lossy().replace('\\', "/")),
                color: None,
            },
        )
        .await
        .unwrap()
        .workspaces
        .into_iter()
        .find(|ws| ws.name == "kinds")
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn a_markdown_pane_needs_a_file_and_shows_it() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, state) = AppState::for_tests();
        let ws = workspace_in(&state, dir.path()).await;
        let notes = dir.path().join("notes.md");
        std::fs::write(&notes, "# Plan\n\n- first\n").unwrap();

        // Without a file there is nothing to show.
        let missing = create_pane(
            &state,
            CreatePaneArgs {
                workspace: Some(ws.clone()),
                kind: Some("markdown".into()),
                ..CreatePaneArgs::default()
            },
        )
        .await;
        assert!(
            matches!(missing, Err(WorkspaceError::NeedsFile)),
            "{missing:?}"
        );

        // A directory is not a file, even though every other kind wants one.
        let dir_given = create_pane(
            &state,
            CreatePaneArgs {
                workspace: Some(ws.clone()),
                kind: Some("markdown".into()),
                cwd: Some(dir.path().to_string_lossy().into_owned()),
                ..CreatePaneArgs::default()
            },
        )
        .await;
        assert!(
            matches!(dir_given, Err(WorkspaceError::InvalidFile(_))),
            "{dir_given:?}"
        );

        let created = create_pane(
            &state,
            CreatePaneArgs {
                workspace: Some(ws.clone()),
                kind: Some("markdown".into()),
                cwd: Some(notes.to_string_lossy().into_owned()),
                ..CreatePaneArgs::default()
            },
        )
        .await
        .unwrap();
        let shown = pane_content(
            &state,
            PaneContentArgs {
                pane: created.pane.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(shown.text, "# Plan\n\n- first\n");
        assert!(!shown.truncated);
        assert!(shown.modified_at > 0);
        assert!(shown.path.ends_with("/notes.md"), "{}", shown.path);
    }

    #[tokio::test]
    async fn content_is_only_for_markdown_panes() {
        // The read is tied to a markdown pane on purpose: a terminal pane's cwd
        // is a directory, and "read any path" is not a capability the pipe offers.
        let dir = tempfile::tempdir().unwrap();
        let (_db, state) = AppState::for_tests();
        let ws = workspace_in(&state, dir.path()).await;
        let terminal = create_pane(
            &state,
            CreatePaneArgs {
                workspace: Some(ws),
                ..CreatePaneArgs::default()
            },
        )
        .await
        .unwrap();
        let err = pane_content(
            &state,
            PaneContentArgs {
                pane: terminal.pane,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, WorkspaceError::NotMarkdown(_)), "{err:?}");
    }

    #[tokio::test]
    async fn a_diff_pane_sits_in_a_directory_like_a_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, state) = AppState::for_tests();
        let ws = workspace_in(&state, dir.path()).await;
        let created = create_pane(
            &state,
            CreatePaneArgs {
                workspace: Some(ws),
                kind: Some("diff".into()),
                ..CreatePaneArgs::default()
            },
        )
        .await
        .unwrap();
        let panes = list_panes(&state, ListPanesArgs::default()).await.unwrap();
        let pane = panes.panes.iter().find(|p| p.id == created.pane).unwrap();
        assert_eq!(pane.kind, "diff");
        assert_eq!(pane.cwd, dir.path().to_string_lossy().replace('\\', "/"));
    }
}
