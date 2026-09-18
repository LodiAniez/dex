//! Moving a pane by dragging it onto another (issue #31). Dropped on one of
//! the target's edges, the pane leaves its place - its parent split closes up,
//! as when a pane is closed - and splits the target on that side, taking half.
//! Dropped in the middle, the two trade places. Pure: no I/O, no database.

use dex_protocol::workspace::{DropSide, Layout, SplitDir};

use super::layout::{self, Closed};

/// `layout` with `moving` dropped on `target` at `side`. `None` if either pane
/// is not in the tree, or they are the same pane.
pub fn move_pane(layout: &Layout, moving: &str, target: &str, side: DropSide) -> Option<Layout> {
    let present = layout::leaves(layout);
    let here = |id: &str| present.iter().any(|leaf| leaf == id);
    if moving == target || !here(moving) || !here(target) {
        return None;
    }
    let (dir, moving_first) = match side {
        DropSide::Center => return layout::swap_panes(layout, moving, target),
        DropSide::Left => (SplitDir::Horizontal, true),
        DropSide::Right => (SplitDir::Horizontal, false),
        DropSide::Top => (SplitDir::Vertical, true),
        DropSide::Bottom => (SplitDir::Vertical, false),
    };
    let Closed::Remaining(without) = layout::close_pane(layout, moving) else {
        return None;
    };
    let (a, b) = if moving_first {
        (layout::leaf(moving), layout::leaf(target))
    } else {
        (layout::leaf(target), layout::leaf(moving))
    };
    replace_leaf(&without, target, &layout::split_of(dir, 0.5, a, b))
}

/// `layout` with the leaf showing `target` replaced by `with`.
fn replace_leaf(layout: &Layout, target: &str, with: &Layout) -> Option<Layout> {
    match layout {
        Layout::Leaf { pane_id } if pane_id == target => Some(with.clone()),
        Layout::Leaf { .. } => None,
        Layout::Split { dir, ratio, a, b } => {
            if let Some(new_a) = replace_leaf(a, target, with) {
                return Some(layout::split_of(*dir, *ratio, new_a, (**b).clone()));
            }
            replace_leaf(b, target, with)
                .map(|new_b| layout::split_of(*dir, *ratio, (**a).clone(), new_b))
        }
    }
}
