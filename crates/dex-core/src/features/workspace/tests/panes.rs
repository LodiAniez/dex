//! Pane tree commands: split, close, focus, swap, and layouts.

use dex_protocol::workspace::{
    Layout, PaneArgs, SetLayoutArgs, SplitDir, SplitDirection, SplitPaneArgs, SwapPanesArgs,
    WorkspaceArgs, WorkspaceList,
};

use super::named;
use crate::app::AppState;
use crate::features::workspace::{
    WorkspaceError, close_pane, create, cycle_layout, focus_pane, layout, set_layout, split_pane,
    swap_panes,
};

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
        cwd: None,
        label: None,
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
