//! Command tests against a real `AppState` over a temp database.

use dex_protocol::workspace::{
    CreateWorkspaceArgs, Layout, PaneArgs, RecolorWorkspaceArgs, RenameWorkspaceArgs,
    ReorderWorkspacesArgs, SetLayoutArgs, SplitDir, SplitDirection, SplitPaneArgs, SwapPanesArgs,
    WorkspaceArgs, WorkspaceList,
};

use super::{
    WorkspaceError, close_pane, create, cycle_layout, delete, focus_pane, layout, list, logic,
    recolor, rename, reorder, set_layout, split_pane, store, swap_panes, switch,
};
use crate::app::AppState;

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

fn pane_ids(list: &WorkspaceList) -> Vec<String> {
    list.workspaces[0]
        .panes
        .iter()
        .map(|p| p.id.clone())
        .collect()
}

fn split_args(pane: &str, direction: SplitDirection) -> SplitPaneArgs {
    SplitPaneArgs {
        pane: pane.into(),
        direction,
    }
}

fn pane(id: &str) -> PaneArgs {
    PaneArgs { pane: id.into() }
}

/// A fresh workspace; returns its id and its only pane's id.
async fn one_workspace(state: &AppState) -> (String, String) {
    let list = create(state, named("api")).await.unwrap();
    let ws = &list.workspaces[0];
    (ws.id.clone(), ws.panes[0].id.clone())
}

#[tokio::test]
async fn splitting_adds_a_pane_in_the_same_folder_that_takes_focus() {
    let (_dir, state) = AppState::for_tests();
    let (_, first) = one_workspace(&state).await;

    let list = split_pane(&state, split_args(&first, SplitDirection::Down))
        .await
        .unwrap();

    let ws = &list.workspaces[0];
    assert_eq!(ws.panes.len(), 2);
    assert_eq!(ws.panes[1].cwd, ws.panes[0].cwd);
    assert_eq!(ws.active_pane.as_ref(), Some(&ws.panes[1].id));
    assert!(matches!(
        &ws.layout,
        Some(Layout::Split {
            dir: SplitDir::Vertical,
            ..
        })
    ));
}

#[tokio::test]
async fn four_tiled_panes_survive_closing_the_second_with_the_tree_reparented() {
    // PRD M2 acceptance, at the command level.
    let (_dir, state) = AppState::for_tests();
    let (ws_id, first) = one_workspace(&state).await;
    let mut list = split_pane(&state, split_args(&first, SplitDirection::Right))
        .await
        .unwrap();
    for _ in 0..2 {
        let last = pane_ids(&list).pop().unwrap();
        list = split_pane(&state, split_args(&last, SplitDirection::Right))
            .await
            .unwrap();
    }
    // Hand-made splits match no preset, so five cycles land on the fifth: tiled.
    for _ in 0..5 {
        list = cycle_layout(
            &state,
            WorkspaceArgs {
                workspace: ws_id.clone(),
            },
        )
        .await
        .unwrap();
    }
    let ids = pane_ids(&list);
    assert_eq!(
        list.workspaces[0].layout,
        layout::preset(layout::Preset::Tiled, &ids)
    );

    let after = close_pane(&state, pane(&ids[1])).await.unwrap();
    let ws = &after.workspaces[0];
    assert_eq!(ws.panes.len(), 3);
    assert_eq!(
        layout::leaves(ws.layout.as_ref().unwrap()),
        [ids[0].clone(), ids[2].clone(), ids[3].clone()]
    );
}

#[tokio::test]
async fn the_last_pane_cannot_be_closed_and_unknown_panes_are_errors() {
    let (_dir, state) = AppState::for_tests();
    let (_, only) = one_workspace(&state).await;
    assert!(matches!(
        close_pane(&state, pane(&only)).await,
        Err(WorkspaceError::LastPane)
    ));
    assert!(matches!(
        close_pane(&state, pane("gone")).await,
        Err(WorkspaceError::NoSuchPane(_))
    ));
}

#[tokio::test]
async fn closing_the_focused_pane_moves_focus_to_a_remaining_pane() {
    let (_dir, state) = AppState::for_tests();
    let (_, first) = one_workspace(&state).await;
    let list = split_pane(&state, split_args(&first, SplitDirection::Right))
        .await
        .unwrap();
    let second = pane_ids(&list)[1].clone();

    let after = close_pane(&state, pane(&second)).await.unwrap();
    assert_eq!(after.workspaces[0].active_pane, Some(first));
}

#[tokio::test]
async fn swap_and_focus_are_stored() {
    let (_dir, state) = AppState::for_tests();
    let (_, first) = one_workspace(&state).await;
    let list = split_pane(&state, split_args(&first, SplitDirection::Right))
        .await
        .unwrap();
    let second = pane_ids(&list)[1].clone();

    let swapped = swap_panes(
        &state,
        SwapPanesArgs {
            a: first.clone(),
            b: second.clone(),
        },
    )
    .await
    .unwrap();
    let ws = &swapped.workspaces[0];
    assert_eq!(
        layout::leaves(ws.layout.as_ref().unwrap()),
        [second.clone(), first.clone()]
    );
    assert_eq!(ws.active_pane.as_ref(), Some(&first));

    let focused = focus_pane(&state, pane(&second)).await.unwrap();
    assert_eq!(focused.workspaces[0].active_pane, Some(second));
}

#[tokio::test]
async fn set_layout_clamps_ratios_and_rejects_trees_with_other_panes() {
    let (_dir, state) = AppState::for_tests();
    let (ws_id, first) = one_workspace(&state).await;
    let list = split_pane(&state, split_args(&first, SplitDirection::Right))
        .await
        .unwrap();
    let Some(Layout::Split { dir, a, b, .. }) = list.workspaces[0].layout.clone() else {
        panic!("two panes make a split");
    };

    let wide = Layout::Split {
        dir,
        ratio: 5.0,
        a,
        b,
    };
    let saved = set_layout(
        &state,
        SetLayoutArgs {
            workspace: ws_id.clone(),
            layout: wide,
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(saved.workspaces[0].layout, Some(Layout::Split { ratio, .. }) if ratio == 0.9)
    );

    let foreign = Layout::Leaf {
        pane_id: "stranger".into(),
    };
    let rejected = set_layout(
        &state,
        SetLayoutArgs {
            workspace: ws_id,
            layout: foreign,
        },
    )
    .await;
    assert!(matches!(rejected, Err(WorkspaceError::LayoutMismatch)));
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

    let reopened = AppState::open(&dir.path().join("dex.db")).unwrap();
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
