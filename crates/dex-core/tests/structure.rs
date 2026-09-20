//! Structural rules from docs/conventions.md (§1.2 rule 1, §6.3), checked on
//! every test run so drift is caught by CI rather than by review.

// A test crate: panicking is how a test fails.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

#[test]
fn platform_never_imports_features() {
    for file in rust_files(&src_dir().join("platform")) {
        let text = fs::read_to_string(&file).unwrap();
        // Comments may name the rule itself; only code lines count.
        let offending = text
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//"))
            .find(|line| line.contains("crate::features"));
        assert!(
            offending.is_none(),
            "{} refers to crate::features; platform/ must never depend on features/: {:?}",
            file.display(),
            offending
        );
    }
}

#[test]
fn every_slice_has_mod_and_commands() {
    for entry in fs::read_dir(src_dir().join("features")).unwrap() {
        let slice = entry.unwrap().path();
        if !slice.is_dir() {
            continue;
        }
        for required in ["mod.rs", "commands.rs"] {
            assert!(
                slice.join(required).is_file(),
                "slice {} is missing {required}",
                slice.display()
            );
        }
    }
}

/// The watchdog is the only thing that wakes an agent for messages waiting
/// (issue #58), and it must end the departed first: an agent whose Claude Code
/// was quit still looks idle, and its pane is a bare shell that would *run* the
/// nudge. Neither can be seen from a test over a temp database - no pane there
/// has a shell, and no process table is read - so both are checked here.
#[test]
fn the_sweep_ends_the_departed_before_it_wakes_anyone() {
    let sweep = fs::read_to_string(src_dir().join("features/agent/watchdog.rs")).unwrap();
    let departed = sweep
        .find("presence::end_the_departed")
        .expect("the sweep ends agents whose Claude Code has gone");
    let waking = sweep
        .find("waking::wake_waiting")
        .expect("the sweep wakes agents with messages waiting (issue #58)");
    assert!(
        departed < waking,
        "the sweep must end the departed before it types at anyone: their pane is a bare shell"
    );
}

#[test]
fn no_utils_files_exist() {
    for file in rust_files(&src_dir()) {
        assert_ne!(
            file.file_name().and_then(|n| n.to_str()),
            Some("utils.rs"),
            "utils.rs is banned (conventions §3): {}",
            file.display()
        );
    }
}
