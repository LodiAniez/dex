//! Removing a worktree: found wherever it was made and however it has moved
//! on since, and forgotten only once nothing of it is left.

use std::path::{Path, PathBuf};

use super::worktrees::{git, on_branch, registered, removing, workspace_at};
use super::{path_of, repo_at};
use crate::app::AppState;
use crate::features::repo::model::RepoError;
use crate::features::repo::{add_worktree, remove_worktree};

#[tokio::test]
async fn a_worktree_is_removed_by_its_branch_wherever_it_was_made() {
    // Some worktrees went inside their workspace, some to `worktree_base`,
    // and older ones were made before either rule: all come out by branch.
    let (repo, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let old: PathBuf = elsewhere.path().join("api").join("old-place");
    git(
        repo.path(),
        &["worktree", "add", &path_of(&old), "-b", "old/place"],
    );

    let after = remove_worktree(&state, removing("old/place"))
        .await
        .unwrap();

    assert_eq!(after.worktrees.len(), 1, "only the main checkout is left");
    assert!(!old.exists(), "and the folder went with it");
}

#[tokio::test]
async fn a_worktree_that_has_left_its_branch_is_still_removed_by_it() {
    // The agent switched branch, or is mid-rebase with none: Dex recorded
    // where it made the worktree, and finds it there.
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;
    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();
    let checkout = root.path().join(".dex/worktrees/api/fix-login");
    git(&checkout, &["checkout", "--detach"]);

    let after = remove_worktree(&state, removing("fix/login"))
        .await
        .unwrap();

    assert_eq!(after.worktrees.len(), 1, "only the main checkout is left");
    assert!(!checkout.exists());
}

#[tokio::test]
async fn removing_a_worktree_nobody_knows_of_says_so() {
    let repo = tempfile::tempdir().unwrap();
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;

    // The main checkout's branch: never a worktree to remove.
    let missing = remove_worktree(&state, removing("main")).await;
    assert!(
        matches!(missing, Err(RepoError::NoSuchWorktree { ref branch, .. }) if branch == "main"),
        "{missing:?}"
    );
}

#[tokio::test]
async fn a_worktree_git_and_the_disk_have_both_lost_is_forgotten() {
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;
    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();
    let checkout = root.path().join(".dex/worktrees/api/fix-login");
    // Taken away by hand, behind Dex's back.
    git(
        repo.path(),
        &["worktree", "remove", "--force", &path_of(&checkout)],
    );

    let after = remove_worktree(&state, removing("fix/login")).await;
    assert!(after.is_ok(), "all that was left was the record: {after:?}");
    // And the record is gone with it.
    let again = remove_worktree(&state, removing("fix/login")).await;
    assert!(
        matches!(again, Err(RepoError::NoSuchWorktree { .. })),
        "{again:?}"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn a_worktree_is_found_however_its_workspace_root_was_written() {
    // The root as someone typed it, in other letters; git prints the folder's
    // own. The worktree has left its branch, so only the record finds it.
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let typed = path_of(root.path()).to_lowercase();
    let workspace = workspace_at(&state, Path::new(&typed)).await;
    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();
    let checkout = root.path().join(".dex/worktrees/api/fix-login");
    git(&checkout, &["checkout", "--detach"]);

    let after = remove_worktree(&state, removing("fix/login"))
        .await
        .unwrap();

    assert_eq!(after.worktrees.len(), 1, "taken away, not merely forgotten");
    assert!(!checkout.exists());
}

#[tokio::test]
async fn a_worktree_git_has_lost_but_whose_folder_remains_says_where_it_is() {
    // What a removal that failed part-way leaves - git gives up on the
    // worktree first and then cannot delete a file something holds open - made
    // here by taking away git's own record of it.
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;
    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();
    let checkout: PathBuf = root.path().join(".dex/worktrees/api/fix-login");
    std::fs::remove_dir_all(repo.path().join(".git/worktrees/fix-login")).unwrap();

    let left = remove_worktree(&state, removing("fix/login")).await;
    assert!(
        matches!(left, Err(RepoError::WorktreeLeftBehind { ref path }) if path.ends_with("fix-login")),
        "{left:?}"
    );
    assert!(
        checkout.exists(),
        "the folder is the owner's to delete, not Dex's"
    );

    // Once they have, the record goes too.
    std::fs::remove_dir_all(&checkout).unwrap();
    assert!(remove_worktree(&state, removing("fix/login")).await.is_ok());
    assert!(matches!(
        remove_worktree(&state, removing("fix/login")).await,
        Err(RepoError::NoSuchWorktree { .. })
    ));
}
