//! `pane.move`: a pane dragged by its header and dropped on another pane - on
//! an edge to sit beside it there, in the middle to trade places with it.

use dex_protocol::workspace::{
    CreateWorkspaceArgs, DropSide, Layout, MovePaneArgs, SplitDir, SplitDirection, SplitPaneArgs,
};

use crate::app::AppState;
use crate::features::workspace::docking::move_pane;
use crate::features::workspace::{create, move_pane as move_command, split_pane};

fn leaf(id: &str) -> Layout {
    Layout::Leaf { pane_id: id.into() }
}

fn split(dir: SplitDir, ratio: f64, a: Layout, b: Layout) -> Layout {
    Layout::Split {
        dir,
        ratio,
        a: Box::new(a),
        b: Box::new(b),
    }
}

/// p1 | (p2 over p3)
fn three() -> Layout {
    split(
        SplitDir::Horizontal,
        0.5,
        leaf("p1"),
        split(SplitDir::Vertical, 0.5, leaf("p2"), leaf("p3")),
    )
}

#[test]
fn dropped_on_an_edge_it_sits_beside_the_target_on_that_side() {
    // p3 dropped on p1's left edge: out of the stack, which closes up, and in left of p1.
    assert_eq!(
        move_pane(&three(), "p3", "p1", DropSide::Left),
        Some(split(
            SplitDir::Horizontal,
            0.5,
            split(SplitDir::Horizontal, 0.5, leaf("p3"), leaf("p1")),
            leaf("p2"),
        ))
    );
    // On p1's bottom edge: stacked under it.
    assert_eq!(
        move_pane(&three(), "p3", "p1", DropSide::Bottom),
        Some(split(
            SplitDir::Horizontal,
            0.5,
            split(SplitDir::Vertical, 0.5, leaf("p1"), leaf("p3")),
            leaf("p2"),
        ))
    );
}

#[test]
fn dropped_on_the_top_or_right_edge_it_comes_first_or_second_accordingly() {
    assert_eq!(
        move_pane(&three(), "p1", "p2", DropSide::Top),
        Some(split(
            SplitDir::Vertical,
            0.5,
            split(SplitDir::Vertical, 0.5, leaf("p1"), leaf("p2")),
            leaf("p3"),
        ))
    );
    assert_eq!(
        move_pane(&three(), "p1", "p3", DropSide::Right),
        Some(split(
            SplitDir::Vertical,
            0.5,
            leaf("p2"),
            split(SplitDir::Horizontal, 0.5, leaf("p3"), leaf("p1")),
        ))
    );
}

#[test]
fn dropped_in_the_middle_the_two_trade_places() {
    assert_eq!(
        move_pane(&three(), "p1", "p3", DropSide::Center),
        Some(split(
            SplitDir::Horizontal,
            0.5,
            leaf("p3"),
            split(SplitDir::Vertical, 0.5, leaf("p2"), leaf("p1")),
        ))
    );
}

#[test]
fn a_pane_moved_beside_its_own_sibling_keeps_every_pane_once() {
    let moved = move_pane(&three(), "p3", "p2", DropSide::Top).unwrap();
    let mut shown = crate::features::workspace::leaves_of(&moved);
    shown.sort();
    assert_eq!(shown, ["p1", "p2", "p3"]);
}

#[test]
fn nothing_moves_onto_itself_or_to_a_pane_that_is_not_there() {
    assert_eq!(move_pane(&three(), "p1", "p1", DropSide::Left), None);
    assert_eq!(move_pane(&three(), "p1", "nope", DropSide::Left), None);
    assert_eq!(move_pane(&three(), "nope", "p1", DropSide::Left), None);
    assert_eq!(move_pane(&leaf("p1"), "p1", "p1", DropSide::Center), None);
}

#[tokio::test]
async fn the_move_is_stored_and_the_moved_pane_has_focus() {
    let (_dir, state) = AppState::for_tests();
    let list = create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let first = list.workspaces[0].panes[0].id.clone();
    let list = split_pane(
        &state,
        SplitPaneArgs {
            pane: first.clone(),
            direction: SplitDirection::Right,
            cwd: None,
            label: None,
            kind: None,
            runtime: None,
        },
    )
    .await
    .unwrap();
    let second = list.workspaces[0].active_pane.clone().unwrap();

    let moved = move_command(
        &state,
        MovePaneArgs {
            pane: second.clone(),
            target: first.clone(),
            side: DropSide::Top,
        },
    )
    .await
    .unwrap();

    let ws = &moved.workspaces[0];
    assert_eq!(
        ws.layout,
        Some(split(SplitDir::Vertical, 0.5, leaf(&second), leaf(&first)))
    );
    assert_eq!(ws.active_pane.as_deref(), Some(second.as_str()));
}

#[tokio::test]
async fn a_move_onto_itself_is_refused() {
    let (_dir, state) = AppState::for_tests();
    let list = create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let first = list.workspaces[0].panes[0].id.clone();
    let refused = move_command(
        &state,
        MovePaneArgs {
            pane: first.clone(),
            target: first,
            side: DropSide::Left,
        },
    )
    .await;
    assert!(refused.is_err());
}
