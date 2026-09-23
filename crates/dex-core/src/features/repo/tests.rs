//! Repo and worktree commands against real `git.exe` in temp repositories.
//!
//! These drive the actual subprocess rather than a fake: the whole point of the
//! slice is that git's behaviour and its error text are what they are, and a
//! stub would encode this author's belief about git instead of git.

use std::path::Path;
use std::process::Command;

use dex_protocol::repo::{AddRepoArgs, AddWorktreeArgs, RepoArgs, ScanArgs};

use super::model::RepoError;
use super::{add, add_worktree, list, list_worktrees, scan, status};
use crate::app::AppState;
use crate::platform::proc::GitError;

mod worktree_removal;
mod worktrees;

/// A git repository with one commit, so HEAD and branches exist.
fn repo_at(dir: &Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git should be on PATH for these tests");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&["init", "--initial-branch=main"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-m", "first"]);
}

fn path_of(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

#[tokio::test]
async fn adding_a_repository_registers_it_with_its_branch() {
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();

    let listed = add(
        &state,
        AddRepoArgs {
            path: path_of(work.path()),
            name: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(listed.repos.len(), 1);
    assert_eq!(listed.repos[0].branch.as_deref(), Some("main"));
    assert_eq!(
        listed.repos[0].name,
        work.path().file_name().unwrap().to_string_lossy()
    );
}

#[tokio::test]
async fn adding_the_same_repository_twice_does_not_duplicate_it() {
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    let args = || AddRepoArgs {
        path: path_of(work.path()),
        name: None,
    };

    add(&state, args()).await.unwrap();
    let twice = add(&state, args()).await.unwrap();
    assert_eq!(twice.repos.len(), 1);
}

#[tokio::test]
async fn a_directory_that_is_not_a_repository_is_refused_in_gits_terms() {
    let plain = tempfile::tempdir().unwrap();
    let (_dir, state) = AppState::for_tests();

    let result = add(
        &state,
        AddRepoArgs {
            path: path_of(plain.path()),
            name: None,
        },
    )
    .await;
    assert!(
        matches!(result, Err(RepoError::Git(GitError::NotARepo(_)))),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_missing_directory_is_refused_before_git_is_run() {
    let (_dir, state) = AppState::for_tests();
    let result = add(
        &state,
        AddRepoArgs {
            path: "C:/no/such/place/at/all".into(),
            name: None,
        },
    )
    .await;
    assert!(matches!(result, Err(RepoError::NoSuchDir(_))), "{result:?}");
}

#[tokio::test]
async fn scanning_finds_repositories_without_descending_into_them() {
    let root = tempfile::tempdir().unwrap();
    let one = root.path().join("one");
    let two = root.path().join("nested").join("two");
    std::fs::create_dir_all(&one).unwrap();
    std::fs::create_dir_all(&two).unwrap();
    repo_at(&one);
    repo_at(&two);
    // A repository inside a repository is part of it, not another entry.
    let inner = one.join("vendor");
    std::fs::create_dir_all(&inner).unwrap();
    repo_at(&inner);
    let (_dir, state) = AppState::for_tests();

    let found = scan(
        &state,
        ScanArgs {
            path: path_of(root.path()),
            depth: None,
        },
    )
    .await
    .unwrap();

    let names: Vec<&str> = found.repos.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"one"), "{names:?}");
    assert!(names.contains(&"two"), "{names:?}");
    assert!(
        !names.contains(&"vendor"),
        "a repo inside a repo is its business: {names:?}"
    );
}

#[tokio::test]
async fn status_counts_the_working_tree() {
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    std::fs::write(work.path().join("README.md"), "changed\n").unwrap();
    std::fs::write(work.path().join("new.txt"), "fresh\n").unwrap();
    let (_dir, state) = AppState::for_tests();
    add(
        &state,
        AddRepoArgs {
            path: path_of(work.path()),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();

    let counts = status(&state, RepoArgs { repo: "api".into() })
        .await
        .unwrap();
    assert_eq!(counts.branch.as_deref(), Some("main"));
    assert_eq!(counts.modified, 1);
    assert_eq!(counts.added, 1);
}

#[tokio::test]
async fn a_branch_name_windows_cannot_store_is_refused_before_git_runs() {
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    add(
        &state,
        AddRepoArgs {
            path: path_of(work.path()),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();

    for branch in ["con", "has space", "bad..name"] {
        let result = add_worktree(
            &state,
            AddWorktreeArgs {
                repo: "api".into(),
                branch: branch.into(),
                workspace: None,
            },
        )
        .await;
        assert!(
            matches!(result, Err(RepoError::InvalidBranch { .. })),
            "{branch:?} should be refused, got {result:?}"
        );
    }
}

#[tokio::test]
async fn an_unknown_repository_names_what_was_asked_for() {
    let (_dir, state) = AppState::for_tests();
    let result = list_worktrees(
        &state,
        RepoArgs {
            repo: "nope".into(),
        },
    )
    .await;
    assert!(
        matches!(result, Err(RepoError::NoSuchRepo(ref target)) if target == "nope"),
        "{result:?}"
    );
    assert!(list(&state).await.unwrap().repos.is_empty());
}

#[tokio::test]
async fn the_diff_shows_working_tree_changes_and_then_staged_ones() {
    use dex_protocol::repo::DiffArgs;

    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    let path = path_of(work.path());

    let clean = super::diff(
        &state,
        DiffArgs {
            path: path.clone(),
            staged: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(clean.text, "", "a clean tree has no diff");
    assert_eq!(clean.branch.as_deref(), Some("main"));

    std::fs::write(work.path().join("README.md"), "hello\nworld\n").unwrap();
    let unstaged = super::diff(
        &state,
        DiffArgs {
            path: path.clone(),
            staged: false,
        },
    )
    .await
    .unwrap();
    assert!(unstaged.text.contains("+world"), "{}", unstaged.text);
    assert!(!unstaged.truncated);

    // Staging moves the change from one diff to the other.
    let ok = Command::new("git")
        .args(["add", "."])
        .current_dir(work.path())
        .status()
        .unwrap();
    assert!(ok.success());
    let after_add = super::diff(
        &state,
        DiffArgs {
            path: path.clone(),
            staged: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(after_add.text, "");
    let staged = super::diff(&state, DiffArgs { path, staged: true })
        .await
        .unwrap();
    assert!(staged.text.contains("+world"), "{}", staged.text);
}

#[tokio::test]
async fn a_diff_outside_any_repository_says_so() {
    use dex_protocol::repo::DiffArgs;

    let plain = tempfile::tempdir().unwrap();
    let (_dir, state) = AppState::for_tests();
    let err = super::diff(
        &state,
        DiffArgs {
            path: path_of(plain.path()),
            staged: false,
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, RepoError::Git(GitError::NotARepo(_))),
        "{err:?}"
    );
}
