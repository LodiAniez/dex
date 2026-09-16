//! Pure rules for the context store: which keys are legal, where a key lands
//! on disk, and how tags filter (docs/prd.md §10.1). No I/O, no database.

use std::path::{Path, PathBuf};

use crate::platform::paths;

/// Longest key the store accepts (PRD §10.1).
pub const MAX_KEY: usize = 128;

/// Why a key was refused. The router turns each into a repair string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadKey {
    Empty,
    TooLong,
    /// A character outside `[a-z0-9._/-]`, including any uppercase letter.
    Character,
    /// An empty segment: a leading, trailing, or doubled `/`.
    Segment,
    /// A `.` or `..` segment, which would walk out of the mirror directory.
    Traversal,
    /// A segment Windows cannot store under that name.
    Reserved,
}

impl BadKey {
    /// What the caller should do instead.
    pub fn repair(self) -> &'static str {
        match self {
            Self::Empty => "Give a key such as `auth/jwt-expiry`.",
            Self::TooLong => "Shorten the key to 128 characters or fewer.",
            Self::Character => {
                "Use only lowercase letters, digits, and `. _ - /`, as in `auth/jwt-expiry`."
            }
            Self::Segment => "Remove the leading, trailing, or doubled `/`.",
            Self::Traversal => "Drop the `.` or `..` segment; a key is a name, not a path.",
            Self::Reserved => {
                "Rename that part of the key: Windows reserves names like `con` and `nul`."
            }
        }
    }
}

/// Checks a key against PRD §10.1, and against what the disk mirror can store.
///
/// The mirror turns a key into a file path under the workspace, so validation
/// is a security boundary as much as a style rule: `../` or an absolute-looking
/// segment would otherwise write outside `.dex/context/`.
pub fn check_key(key: &str) -> Result<(), BadKey> {
    if key.is_empty() {
        return Err(BadKey::Empty);
    }
    if key.len() > MAX_KEY {
        return Err(BadKey::TooLong);
    }
    if !key.bytes().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-' | b'/')
    }) {
        return Err(BadKey::Character);
    }
    for segment in key.split('/') {
        if segment.is_empty() {
            return Err(BadKey::Segment);
        }
        if segment == "." || segment == ".." {
            return Err(BadKey::Traversal);
        }
        if paths::is_reserved_name(segment) {
            return Err(BadKey::Reserved);
        }
    }
    Ok(())
}

/// Where a key is mirrored: `<root>/.dex/context/<key>.md`, one directory per
/// `/` in the key. Call `check_key` first; this assumes a checked key.
pub fn mirror_path(root: &Path, key: &str) -> PathBuf {
    let mut path = root.join(".dex").join("context");
    let mut segments = key.split('/').peekable();
    while let Some(segment) = segments.next() {
        // `.md` is appended, not set: `set_extension` would turn the key
        // `build.notes` into the file `build.md` and collide with `build`.
        match segments.peek() {
            Some(_) => path.push(segment),
            None => path.push(format!("{segment}.md")),
        }
    }
    path
}

/// Where the human-readable event log is mirrored.
pub fn activity_path(root: &Path) -> PathBuf {
    root.join(".dex").join("activity.log")
}

/// Whether a comma-separated tag list contains `tag`, ignoring spacing and case.
pub fn has_tag(tags: Option<&str>, tag: &str) -> bool {
    let wanted = tag.trim();
    tags.is_some_and(|tags| {
        tags.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case(wanted))
    })
}

/// Tags as stored: trimmed, empties dropped, `None` when nothing is left.
pub fn clean_tags(tags: Option<&str>) -> Option<String> {
    let cleaned: Vec<&str> = tags?
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    (!cleaned.is_empty()).then(|| cleaned.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_keys_are_accepted() {
        for key in ["auth", "auth/jwt-expiry", "build.notes", "a/b/c_d", "x1"] {
            assert_eq!(check_key(key), Ok(()), "{key}");
        }
    }

    #[test]
    fn uppercase_and_stray_characters_are_refused() {
        assert_eq!(check_key("Auth"), Err(BadKey::Character));
        assert_eq!(check_key("auth key"), Err(BadKey::Character));
        assert_eq!(check_key("auth:key"), Err(BadKey::Character));
        assert_eq!(check_key("auth\\key"), Err(BadKey::Character));
    }

    #[test]
    fn a_key_can_never_climb_out_of_the_mirror_directory() {
        assert_eq!(check_key(".."), Err(BadKey::Traversal));
        assert_eq!(check_key("../secrets"), Err(BadKey::Traversal));
        assert_eq!(check_key("a/../../b"), Err(BadKey::Traversal));
        assert_eq!(check_key("a/./b"), Err(BadKey::Traversal));
        assert_eq!(check_key("/abs"), Err(BadKey::Segment));
        assert_eq!(check_key("a//b"), Err(BadKey::Segment));
        assert_eq!(check_key("trailing/"), Err(BadKey::Segment));
    }

    #[test]
    fn keys_windows_cannot_store_are_refused() {
        assert_eq!(check_key("con"), Err(BadKey::Reserved));
        assert_eq!(check_key("logs/nul"), Err(BadKey::Reserved));
        assert_eq!(check_key("com1/x"), Err(BadKey::Reserved));
    }

    #[test]
    fn empty_and_overlong_keys_are_refused() {
        assert_eq!(check_key(""), Err(BadKey::Empty));
        assert_eq!(check_key(&"a".repeat(MAX_KEY)), Ok(()));
        assert_eq!(check_key(&"a".repeat(MAX_KEY + 1)), Err(BadKey::TooLong));
    }

    #[test]
    fn every_refusal_says_what_to_do_instead() {
        for bad in [
            BadKey::Empty,
            BadKey::TooLong,
            BadKey::Character,
            BadKey::Segment,
            BadKey::Traversal,
            BadKey::Reserved,
        ] {
            assert!(!bad.repair().is_empty(), "{bad:?}");
        }
    }

    #[test]
    fn a_key_becomes_one_markdown_file_per_namespace() {
        let root = Path::new("C:/ws");
        assert_eq!(
            mirror_path(root, "auth/jwt-expiry"),
            Path::new("C:/ws/.dex/context/auth/jwt-expiry.md")
        );
        assert_eq!(
            mirror_path(root, "notes"),
            Path::new("C:/ws/.dex/context/notes.md")
        );
    }

    #[test]
    fn a_dotted_key_keeps_its_name_and_gains_the_extension() {
        // set_extension would otherwise replace `.notes` with `.md`.
        assert_eq!(
            mirror_path(Path::new("C:/ws"), "build.notes"),
            Path::new("C:/ws/.dex/context/build.notes.md")
        );
    }

    #[test]
    fn tag_matching_ignores_spacing_and_case() {
        assert!(has_tag(Some("api, Auth ,db"), "auth"));
        assert!(has_tag(Some("api"), "api"));
        assert!(!has_tag(Some("api"), "ap"));
        assert!(!has_tag(None, "api"));
    }

    #[test]
    fn tags_are_stored_tidied_or_not_at_all() {
        assert_eq!(
            clean_tags(Some(" api , auth ")).as_deref(),
            Some("api,auth")
        );
        assert_eq!(clean_tags(Some(" , ")), None);
        assert_eq!(clean_tags(None), None);
    }
}
