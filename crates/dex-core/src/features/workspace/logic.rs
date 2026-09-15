//! Pure functions for the workspace slice: no I/O, no database, no clock.

use std::collections::HashSet;

use dex_protocol::workspace::Layout;

use super::layout;

/// Colors offered by the picker; new workspaces take them in turn.
/// Mirrored in `app/src/features/workspaces/palette.ts`.
pub const PALETTE: [&str; 8] = [
    "#4f8cff", "#3ecf8e", "#f5a524", "#f25f5c", "#a970ff", "#2ec4d6", "#f178b6", "#8a93a6",
];

/// Longest workspace name accepted, in characters.
const MAX_NAME_CHARS: usize = 64;

/// The stored form of a color, `#rrggbb` lowercase; `None` if `input` isn't one.
pub fn normalize_color(input: &str) -> Option<String> {
    let hex = input.trim().strip_prefix('#')?;
    let valid = hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit());
    valid.then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// The color a new workspace gets when `existing` workspaces already exist.
pub fn default_color(existing: usize) -> String {
    PALETTE[existing % PALETTE.len()].to_owned()
}

/// `input` trimmed, if it is a usable workspace name.
pub fn clean_name(input: &str) -> Option<String> {
    let name = input.trim();
    let usable = !name.is_empty() && name.chars().count() <= MAX_NAME_CHARS;
    usable.then(|| name.to_owned())
}

/// "Workspace N" with the smallest N not already taken.
pub fn default_name(taken: &[String]) -> String {
    let mut n = 1;
    loop {
        let candidate = format!("Workspace {n}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// New sort indexes for `requested`, if it names every id in `existing` exactly once.
pub fn reorder(existing: &[String], requested: &[String]) -> Option<Vec<(String, i64)>> {
    let wanted: HashSet<&String> = requested.iter().collect();
    let have: HashSet<&String> = existing.iter().collect();
    if wanted.len() != requested.len() || wanted != have {
        return None;
    }
    Some(requested.iter().cloned().zip(0..).collect())
}

/// The stored layout if it shows exactly the workspace's panes; otherwise a
/// single leaf with the first pane. A tree naming a missing pane would render
/// as blank space, so a stale tree is replaced rather than trusted.
pub fn layout_or_default(layout_json: &str, pane_ids: &[String]) -> Option<Layout> {
    if let Ok(stored) = serde_json::from_str::<Layout>(layout_json) {
        let mut shown = layout::leaves(&stored);
        let mut all = pane_ids.to_vec();
        shown.sort();
        all.sort();
        if shown == all {
            return Some(stored);
        }
    }
    pane_ids.first().map(|id| Layout::Leaf {
        pane_id: id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn colors_are_normalized_to_lowercase_hex() {
        assert_eq!(normalize_color(" #A1B2C3 "), Some("#a1b2c3".into()));
        assert_eq!(normalize_color("a1b2c3"), None);
        assert_eq!(normalize_color("#abc"), None);
        assert_eq!(normalize_color("#gggggg"), None);
    }

    #[test]
    fn default_colors_cycle_through_the_palette() {
        assert_eq!(default_color(0), PALETTE[0]);
        assert_eq!(default_color(PALETTE.len() + 1), PALETTE[1]);
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(clean_name("  api  "), Some("api".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(65)), None);
    }

    #[test]
    fn default_name_takes_the_first_free_number() {
        assert_eq!(default_name(&[]), "Workspace 1");
        assert_eq!(
            default_name(&ids(&["Workspace 1", "Workspace 3"])),
            "Workspace 2"
        );
    }

    #[test]
    fn reorder_requires_every_id_exactly_once() {
        let existing = ids(&["a", "b", "c"]);
        assert_eq!(
            reorder(&existing, &ids(&["c", "a", "b"])),
            Some(vec![("c".into(), 0), ("a".into(), 1), ("b".into(), 2)])
        );
        assert_eq!(reorder(&existing, &ids(&["a", "b"])), None);
        assert_eq!(reorder(&existing, &ids(&["a", "a", "b"])), None);
        assert_eq!(reorder(&existing, &ids(&["a", "b", "x"])), None);
    }

    #[test]
    fn a_stale_or_unreadable_layout_falls_back_to_the_first_pane() {
        let leaf = |id: &str| Layout::Leaf { pane_id: id.into() };
        let stored = serde_json::to_string(&leaf("p1")).unwrap();
        assert_eq!(layout_or_default(&stored, &ids(&["p1"])), Some(leaf("p1")));
        assert_eq!(layout_or_default(&stored, &ids(&["p2"])), Some(leaf("p2")));
        assert_eq!(
            layout_or_default("not json", &ids(&["p3"])),
            Some(leaf("p3"))
        );
        assert_eq!(layout_or_default(&stored, &[]), None);
    }
}
