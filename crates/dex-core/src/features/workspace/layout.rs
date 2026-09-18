//! Pane tree operations (docs/prd.md §7.2): split, close, swap, ratio
//! clamping, and the five preset layouts. Pure: no I/O, no database.

use dex_protocol::workspace::{Layout, SplitDir};

/// A pane squeezed below 10% of its split is unusable; above 90% hides its sibling.
const MIN_RATIO: f64 = 0.1;
const MAX_RATIO: f64 = 0.9;

/// The preset layouts, in the order `Ctrl+Shift+Space` cycles through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// All panes side by side.
    EvenHorizontal,
    /// All panes stacked.
    EvenVertical,
    /// One large pane on the left, the rest stacked on the right.
    MainVertical,
    /// One large pane on top, the rest side by side below.
    MainHorizontal,
    /// A grid.
    Tiled,
}

impl Preset {
    /// Whether the preset still makes sense as panes are added: one large pane
    /// with the rest beside or below it, or a grid, does; six panes in one even
    /// row or column does not. A spawn keeps only these (`arrange.rs`).
    pub fn grows_well(self) -> bool {
        matches!(
            self,
            Preset::MainVertical | Preset::MainHorizontal | Preset::Tiled
        )
    }
}

/// Cycling order.
pub const PRESETS: [Preset; 5] = [
    Preset::EvenHorizontal,
    Preset::EvenVertical,
    Preset::MainVertical,
    Preset::MainHorizontal,
    Preset::Tiled,
];

/// Share of the space the main pane gets in the `Main*` presets.
const MAIN_RATIO: f64 = 0.6;

/// What closing a pane leaves behind.
#[derive(Debug, Clone, PartialEq)]
pub enum Closed {
    /// The tree without the pane; its sibling took the parent split's place.
    Remaining(Layout),
    /// The pane was the only one: a workspace always keeps a pane.
    WasLast,
    /// The pane is not in the tree.
    NotFound,
}

/// A single-pane layout.
pub fn leaf(pane_id: &str) -> Layout {
    Layout::Leaf {
        pane_id: pane_id.to_owned(),
    }
}

/// `ratio` limited to 0.1–0.9; NaN becomes an even split.
pub fn clamp_ratio(ratio: f64) -> f64 {
    if ratio.is_nan() {
        0.5
    } else {
        ratio.clamp(MIN_RATIO, MAX_RATIO)
    }
}

fn split(dir: SplitDir, ratio: f64, a: Layout, b: Layout) -> Layout {
    split_of(dir, ratio, a, b)
}

/// A split of `a` and `b`, `a` taking `ratio` of the space (clamped).
pub fn split_of(dir: SplitDir, ratio: f64, a: Layout, b: Layout) -> Layout {
    Layout::Split {
        dir,
        ratio: clamp_ratio(ratio),
        a: Box::new(a),
        b: Box::new(b),
    }
}

/// Pane ids in reading order (left to right, top to bottom).
pub fn leaves(layout: &Layout) -> Vec<String> {
    match layout {
        Layout::Leaf { pane_id } => vec![pane_id.clone()],
        Layout::Split { a, b, .. } => {
            let mut ids = leaves(a);
            ids.extend(leaves(b));
            ids
        }
    }
}

/// Replaces the leaf `target` with a split holding `target` and `new_pane`
/// (PRD §7.2). `None` if `target` is not in the tree.
pub fn split_pane(layout: &Layout, target: &str, new_pane: &str, dir: SplitDir) -> Option<Layout> {
    match layout {
        Layout::Leaf { pane_id } if pane_id == target => {
            Some(split(dir, 0.5, leaf(target), leaf(new_pane)))
        }
        Layout::Leaf { .. } => None,
        Layout::Split {
            dir: d,
            ratio,
            a,
            b,
        } => {
            if let Some(new_a) = split_pane(a, target, new_pane, dir) {
                return Some(split(*d, *ratio, new_a, (**b).clone()));
            }
            split_pane(b, target, new_pane, dir)
                .map(|new_b| split(*d, *ratio, (**a).clone(), new_b))
        }
    }
}

/// Removes `target`; its parent split is replaced by the surviving sibling (PRD §7.2).
pub fn close_pane(layout: &Layout, target: &str) -> Closed {
    let is_target = |node: &Layout| matches!(node, Layout::Leaf { pane_id } if pane_id == target);
    match layout {
        Layout::Leaf { pane_id } if pane_id == target => Closed::WasLast,
        Layout::Leaf { .. } => Closed::NotFound,
        Layout::Split { dir, ratio, a, b } => {
            if is_target(a) {
                return Closed::Remaining((**b).clone());
            }
            if is_target(b) {
                return Closed::Remaining((**a).clone());
            }
            if let Closed::Remaining(new_a) = close_pane(a, target) {
                return Closed::Remaining(split(*dir, *ratio, new_a, (**b).clone()));
            }
            match close_pane(b, target) {
                Closed::Remaining(new_b) => {
                    Closed::Remaining(split(*dir, *ratio, (**a).clone(), new_b))
                }
                _ => Closed::NotFound,
            }
        }
    }
}

/// Exchanges two panes' positions. `None` unless both are in the tree and distinct.
pub fn swap_panes(layout: &Layout, first: &str, second: &str) -> Option<Layout> {
    let ids = leaves(layout);
    let present = |id: &str| ids.iter().any(|leaf_id| leaf_id == id);
    if first == second || !present(first) || !present(second) {
        return None;
    }
    Some(map_leaves(layout, &|id| {
        if id == first {
            second.to_owned()
        } else if id == second {
            first.to_owned()
        } else {
            id.to_owned()
        }
    }))
}

fn map_leaves(layout: &Layout, rename: &dyn Fn(&str) -> String) -> Layout {
    match layout {
        Layout::Leaf { pane_id } => leaf(&rename(pane_id)),
        Layout::Split { dir, ratio, a, b } => {
            split(*dir, *ratio, map_leaves(a, rename), map_leaves(b, rename))
        }
    }
}

/// The same tree with every ratio clamped (for layouts sent by clients).
pub fn clamp_all(layout: &Layout) -> Layout {
    map_leaves(layout, &|id| id.to_owned())
}

/// `panes` (in creation order) arranged as `preset`. `None` if there are no panes.
pub fn preset(preset: Preset, panes: &[String]) -> Option<Layout> {
    let (first, rest) = panes.split_first()?;
    let leaves_of = |ids: &[String]| ids.iter().map(|id| leaf(id)).collect::<Vec<_>>();
    if rest.is_empty() {
        return Some(leaf(first));
    }
    match preset {
        Preset::EvenHorizontal => chain(SplitDir::Horizontal, leaves_of(panes)),
        Preset::EvenVertical => chain(SplitDir::Vertical, leaves_of(panes)),
        Preset::MainVertical => chain(SplitDir::Vertical, leaves_of(rest))
            .map(|stack| split(SplitDir::Horizontal, MAIN_RATIO, leaf(first), stack)),
        Preset::MainHorizontal => chain(SplitDir::Horizontal, leaves_of(rest))
            .map(|row| split(SplitDir::Vertical, MAIN_RATIO, leaf(first), row)),
        Preset::Tiled => {
            // As many columns as the square root rounds up to: 4 panes make 2x2, 5 make 3+2.
            let columns = (panes.len() as f64).sqrt().ceil() as usize;
            let rows = panes
                .chunks(columns.max(1))
                .map(|row| chain(SplitDir::Horizontal, leaves_of(row)))
                .collect::<Option<Vec<_>>>()?;
            chain(SplitDir::Vertical, rows)
        }
    }
}

/// The next preset in the cycle: if `current` is exactly one preset's output,
/// the one after it; otherwise the first.
pub fn next_preset(current: &Layout, panes: &[String]) -> Option<Layout> {
    let position = preset_of(current, panes).and_then(|p| PRESETS.iter().position(|&q| q == p));
    let next = position.map_or(0, |i| (i + 1) % PRESETS.len());
    preset(PRESETS[next], panes)
}

/// The preset `current` is exactly the output of, if any.
pub fn preset_of(current: &Layout, panes: &[String]) -> Option<Preset> {
    PRESETS
        .iter()
        .copied()
        .find(|&p| preset(p, panes).as_ref() == Some(current))
}

/// `items` in a row (or column) of equal shares: each split gives its first
/// child 1/k of the space, where k counts it and everything after it.
fn chain(dir: SplitDir, items: Vec<Layout>) -> Option<Layout> {
    let mut from_end = items.into_iter().rev();
    let mut tail = from_end.next()?;
    for (index, item) in from_end.enumerate() {
        tail = split(dir, 1.0 / (index as f64 + 2.0), item, tail);
    }
    Some(tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("p{i}")).collect()
    }

    #[test]
    fn a_layout_is_recognised_as_the_preset_it_is_and_only_the_growing_ones_are_kept() {
        let panes = ids(4);
        for p in PRESETS {
            assert_eq!(preset_of(&preset(p, &panes).unwrap(), &panes), Some(p));
        }
        let by_hand = split(SplitDir::Horizontal, 0.33, leaf("p1"), leaf("p2"));
        assert_eq!(preset_of(&by_hand, &ids(2)), None);
        // Six agents in one even row is the mess; a main pane or a grid is not.
        assert!(!Preset::EvenHorizontal.grows_well() && !Preset::EvenVertical.grows_well());
        assert!(
            Preset::MainVertical.grows_well()
                && Preset::MainHorizontal.grows_well()
                && Preset::Tiled.grows_well()
        );
    }

    #[test]
    fn split_replaces_the_leaf_with_a_split_of_old_and_new() {
        let layout = split_pane(&leaf("p1"), "p1", "p2", SplitDir::Horizontal).unwrap();
        assert_eq!(
            layout,
            split(SplitDir::Horizontal, 0.5, leaf("p1"), leaf("p2"))
        );
    }

    #[test]
    fn split_finds_a_nested_leaf_and_keeps_the_rest() {
        let two = split(SplitDir::Horizontal, 0.3, leaf("p1"), leaf("p2"));
        let three = split_pane(&two, "p2", "p3", SplitDir::Vertical).unwrap();
        assert_eq!(leaves(&three), ["p1", "p2", "p3"]);
        assert!(matches!(&three, Layout::Split { ratio, .. } if *ratio == 0.3));
        assert_eq!(split_pane(&two, "nope", "p3", SplitDir::Vertical), None);
    }

    #[test]
    fn closing_the_second_of_four_tiled_panes_reparents_its_sibling() {
        let tiled = preset(Preset::Tiled, &ids(4)).unwrap();
        let Closed::Remaining(after) = close_pane(&tiled, "p2") else {
            panic!("p2 should close");
        };
        assert_eq!(leaves(&after), ["p1", "p3", "p4"]);
        // The top row's split collapsed into p1 itself.
        let Layout::Split { a, .. } = &after else {
            panic!("still a split");
        };
        assert_eq!(**a, leaf("p1"));
    }

    #[test]
    fn the_last_pane_cannot_be_closed_and_unknown_panes_are_reported() {
        assert_eq!(close_pane(&leaf("p1"), "p1"), Closed::WasLast);
        assert_eq!(close_pane(&leaf("p1"), "p9"), Closed::NotFound);
    }

    #[test]
    fn swap_exchanges_two_panes_and_nothing_else() {
        let tiled = preset(Preset::Tiled, &ids(4)).unwrap();
        let swapped = swap_panes(&tiled, "p1", "p4").unwrap();
        assert_eq!(leaves(&swapped), ["p4", "p2", "p3", "p1"]);
        assert_eq!(swap_panes(&tiled, "p1", "p1"), None);
        assert_eq!(swap_panes(&tiled, "p1", "p9"), None);
    }

    #[test]
    fn every_preset_shows_every_pane_exactly_once() {
        for n in 1..=9 {
            for p in PRESETS {
                let layout = preset(p, &ids(n)).unwrap();
                assert_eq!(leaves(&layout), ids(n), "{p:?} with {n} panes");
            }
        }
        assert_eq!(preset(Preset::Tiled, &[]), None);
    }

    #[test]
    fn tiled_four_is_a_two_by_two_grid() {
        let row = |a: &str, b: &str| split(SplitDir::Horizontal, 0.5, leaf(a), leaf(b));
        assert_eq!(
            preset(Preset::Tiled, &ids(4)),
            Some(split(
                SplitDir::Vertical,
                0.5,
                row("p1", "p2"),
                row("p3", "p4")
            ))
        );
    }

    #[test]
    fn even_presets_give_equal_shares() {
        let Some(Layout::Split { ratio, b, .. }) = preset(Preset::EvenHorizontal, &ids(3)) else {
            panic!("three panes make a split");
        };
        assert!((ratio - 1.0 / 3.0).abs() < 1e-9);
        assert!(matches!(*b, Layout::Split { ratio, .. } if (ratio - 0.5).abs() < 1e-9));
    }

    #[test]
    fn cycling_visits_all_five_presets_then_wraps() {
        let panes = ids(3);
        let mut layout = leaf("p1");
        let mut seen = Vec::new();
        for _ in 0..6 {
            layout = next_preset(&layout, &panes).unwrap();
            seen.push(layout.clone());
        }
        for (i, p) in PRESETS.iter().enumerate() {
            assert_eq!(seen[i], preset(*p, &panes).unwrap());
        }
        assert_eq!(seen[5], seen[0]);
    }

    #[test]
    fn ratios_are_clamped_and_nan_is_evened_out() {
        assert_eq!(clamp_ratio(0.01), 0.1);
        assert_eq!(clamp_ratio(0.99), 0.9);
        assert_eq!(clamp_ratio(f64::NAN), 0.5);
        let wild = Layout::Split {
            dir: SplitDir::Vertical,
            ratio: 5.0,
            a: Box::new(leaf("p1")),
            b: Box::new(leaf("p2")),
        };
        assert!(matches!(clamp_all(&wild), Layout::Split { ratio, .. } if ratio == 0.9));
    }
}
