//! Pure functions for the workspace slice: no I/O, no database, no clock.

use std::collections::HashSet;

use dex_protocol::pane::Key;
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

/// Longest pane label accepted, in characters.
const MAX_LABEL_CHARS: usize = 32;

/// `input` trimmed, if it is a usable pane label: 1–32 characters and no
/// whitespace, because labels are typed as `--target` values.
pub fn clean_label(input: &str) -> Option<String> {
    let label = input.trim();
    let usable = !label.is_empty()
        && label.chars().count() <= MAX_LABEL_CHARS
        && !label.chars().any(char::is_whitespace);
    usable.then(|| label.to_owned())
}

/// What a target names among `candidates` (id, optional name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Exactly one candidate, at this index.
    One(usize),
    /// None.
    Nothing,
    /// Several, at these indexes.
    Many(Vec<usize>),
}

/// Resolves a target the way PRD §6.3 prescribes: an exact id, else exact
/// names, else case-insensitive names. Several matches are ambiguous.
pub fn resolve(target: &str, candidates: &[(&str, Option<&str>)]) -> Resolved {
    if let Some(index) = candidates.iter().position(|(id, _)| *id == target) {
        return Resolved::One(index);
    }
    let matching = |same: &dyn Fn(&str) -> bool| -> Vec<usize> {
        candidates
            .iter()
            .enumerate()
            .filter(|(_, (_, name))| name.is_some_and(same))
            .map(|(index, _)| index)
            .collect()
    };
    let exact = matching(&|name| name == target);
    let found = if exact.is_empty() {
        matching(&|name| name.to_lowercase() == target.to_lowercase())
    } else {
        exact
    };
    match found.as_slice() {
        [] => Resolved::Nothing,
        [one] => Resolved::One(*one),
        _ => Resolved::Many(found),
    }
}

/// The bytes a named key sends to a shell: what a terminal sends for it.
/// Pane kinds the app can render. `markdown` and `diff` are in the schema but
/// have no renderer yet, so they are not offered.
const KINDS: [&str; 2] = ["terminal", "activity"];

/// The pane kind a request asked for, defaulting to `terminal`.
///
/// An unknown kind is refused rather than stored: a pane whose kind no renderer
/// understands would show as an empty box with no way to tell why.
pub fn pane_kind(requested: Option<&str>) -> Option<&'static str> {
    let Some(requested) = requested else {
        return Some("terminal");
    };
    KINDS
        .into_iter()
        .find(|kind| kind.eq_ignore_ascii_case(requested))
}

pub fn key_bytes(key: Key) -> &'static [u8] {
    match key {
        Key::Enter => b"\r",
        Key::Tab => b"\t",
        Key::Escape => b"\x1b",
        Key::CtrlC => b"\x03",
        Key::Up => b"\x1b[A",
        Key::Down => b"\x1b[B",
        Key::Right => b"\x1b[C",
        Key::Left => b"\x1b[D",
    }
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

    #[test]
    fn labels_are_trimmed_short_and_space_free() {
        assert_eq!(clean_label(" server "), Some("server".into()));
        assert_eq!(clean_label("two words"), None);
        assert_eq!(clean_label(""), None);
        assert_eq!(clean_label(&"x".repeat(33)), None);
    }

    #[test]
    fn targets_resolve_by_id_then_exact_then_case_insensitive_name() {
        let candidates = [("id-1", Some("Api")), ("id-2", Some("api")), ("id-3", None)];
        assert_eq!(resolve("id-3", &candidates), Resolved::One(2));
        assert_eq!(resolve("Api", &candidates), Resolved::One(0));
        assert_eq!(resolve("API", &candidates), Resolved::Many(vec![0, 1]));
        assert_eq!(resolve("web", &candidates), Resolved::Nothing);
    }

    #[test]
    fn keys_send_what_a_terminal_would() {
        assert_eq!(key_bytes(Key::Enter), b"\r");
        assert_eq!(key_bytes(Key::CtrlC), b"\x03");
        assert_eq!(key_bytes(Key::Up), b"\x1b[A");
    }
}
