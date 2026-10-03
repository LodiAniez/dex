//! A registered repository whose folder is no longer there (issue #72).
//!
//! It happened: the owner deleted a checkout and left 30 worktrees of it on
//! disk, and nothing in Dex said a word - every command that needed the
//! repository failed on git's words about a missing directory, and the folders
//! were found by walking the disk by hand.

use dex_protocol::repo::{AddRepoArgs, AddWorktreeArgs, ForgetRepoArgs, PruneWorktreesArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::worktrees::registered;
use super::{path_of, repo_at};
use crate::app::AppState;
use crate::features::repo::model::RepoError;
use crate::features::repo::{add_worktree, forget, list, prune_worktrees, status};
use crate::features::workspace;

fn asking() -> dex_protocol::repo::RepoArgs {
    dex_protocol::repo::RepoArgs { repo: "api".into() }
}

/// What the owner's machine looked like: a registered repository with one
/// worktree recorded, and then the whole repository deleted.
///
/// The temp dirs come back so they are deleted when the test ends - including
/// the one the repository was in, whose folder is already gone, which
/// `TempDir` takes quietly.
type Deleted = (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    AppState,
    String,
);

async fn deleted() -> Deleted {
    let (work, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(work.path());
    let (data, state) = AppState::for_tests();
    registered(&state, work.path()).await;
    workspace::create(
        &state,
        CreateWorkspaceArgs {
            name: Some("rooted".into()),
            root_path: Some(path_of(root.path())),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    add_worktree(
        &state,
        AddWorktreeArgs {
            repo: "api".into(),
            branch: "feat/orphaned".into(),
            workspace: Some("rooted".into()),
        },
    )
    .await
    .unwrap();
    let worktree = path_of(&root.path().join(".dex/worktrees/api/feat-orphaned"));
    // The repository goes; the worktree stays, as it did on the real machine.
    // The handle is kept and dropped as usual: deleting a folder that is
    // already gone fails quietly, which is what `TempDir` does with it.
    std::fs::remove_dir_all(work.path()).unwrap();
    (work, root, data, state, worktree)
}

#[tokio::test]
async fn a_command_that_needs_the_repository_says_the_registration_is_stale() {
    let (_work, _root, _data, state, worktree) = deleted().await;

    let asked = status(&state, asking()).await;

    let Err(RepoError::RepoGone {
        name,
        path,
        worktrees,
    }) = asked
    else {
        panic!("git's words about a directory say nothing about this: {asked:?}");
    };
    assert_eq!(name, "api");
    assert!(!path.is_empty(), "it says where it was registered");
    assert!(
        worktrees
            .iter()
            .any(|at| at.eq_ignore_ascii_case(&worktree)),
        "and where the orphans are: {worktrees:?} should hold {worktree}"
    );
}

#[tokio::test]
async fn the_prune_says_it_too_rather_than_failing_on_git() {
    // The case that found this: the biggest pile of worktrees on the machine
    // belonged to a repository that had gone, and the prune could not see it.
    let (_work, _root, _data, state, _worktree) = deleted().await;

    let pruned = prune_worktrees(
        &state,
        PruneWorktreesArgs {
            repo: "api".into(),
            dry_run: true,
            auto: false,
        },
    )
    .await;

    assert!(
        matches!(pruned, Err(RepoError::RepoGone { .. })),
        "{pruned:?}"
    );
}

#[tokio::test]
async fn the_listing_marks_it_rather_than_calling_it_detached() {
    let (_work, _root, _data, state, _worktree) = deleted().await;

    let listed = list(&state).await.unwrap();

    let only = &listed.repos[0];
    assert!(only.missing, "the listing says it is gone");
    assert_eq!(only.branch, None, "and claims no branch for it");
}

#[tokio::test]
async fn forgetting_it_works_precisely_because_its_folder_is_gone() {
    let (_work, _root, _data, state, worktree) = deleted().await;

    let after = forget(&state, ForgetRepoArgs { repo: "api".into() })
        .await
        .unwrap();

    assert!(after.repos.is_empty(), "the registration went");
    assert!(
        std::path::Path::new(&worktree).is_dir(),
        "and the worktree did not: nothing can judge it, so it is the owner's"
    );
}

#[tokio::test]
async fn a_repository_that_is_still_there_is_untouched_by_any_of_this() {
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_data, state) = AppState::for_tests();
    add(&state, work.path()).await;

    let listed = list(&state).await.unwrap();

    assert!(!listed.repos[0].missing);
    assert_eq!(listed.repos[0].branch.as_deref(), Some("main"));
    assert!(status(&state, asking()).await.is_ok());
}

async fn add(state: &AppState, dir: &std::path::Path) {
    crate::features::repo::add(
        state,
        AddRepoArgs {
            path: path_of(dir),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
}
