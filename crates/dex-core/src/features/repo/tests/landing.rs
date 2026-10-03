//! The automatic prune against a real remote (issue #74): what "landed" means
//! when the merge was a squash, and the two ways a branch can be absent from a
//! remote.
//!
//! The remote here is a bare repository in a temp folder, which `ls-remote`
//! answers for exactly as a server would. A merged pull request is simulated
//! the way GitHub leaves things: the branch is deleted **in the remote**, while
//! the local remote-tracking ref stays until someone fetches with `--prune`.
//! That difference is the whole test - a branch absent because it was never
//! pushed must be kept, and one absent because the remote took it may go.

use std::path::{Path, PathBuf};

use dex_protocol::repo::{KeptBecause, PruneWorktreesArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::worktrees::{git, on_branch, registered};
use super::{path_of, repo_at};
use crate::app::AppState;
use crate::features::repo::{add_worktree, prune_worktrees};
use crate::features::workspace;

fn automatically() -> PruneWorktreesArgs {
    PruneWorktreesArgs {
        repo: "api".into(),
        dry_run: false,
        auto: true,
    }
}

/// The repository, its bare remote, the workspace root, the data dir, the
/// state, and the worktree. The temp dirs are returned because dropping them
/// deletes them.
type Made = (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    AppState,
    PathBuf,
);

/// A repository with a bare `origin`, and a worktree on `branch` whose commits
/// have been pushed there. `hours` is the grace period: 0 for a worktree to be
/// judged cold, the default for one that counts as just used.
async fn with_a_pushed_worktree(branch: &str, hours: u64) -> Made {
    let (repo, root, bare) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    repo_at(repo.path());
    git(bare.path(), &["init", "--bare", "--initial-branch=main"]);
    git(
        repo.path(),
        &["remote", "add", "origin", &path_of(bare.path())],
    );
    git(repo.path(), &["push", "-u", "origin", "main"]);

    let (data, state) = AppState::for_tests_with(|config| {
        config.agents.prune_after_hours = hours;
        config.agents.prune_merged_worktrees = true;
    });
    registered(&state, repo.path()).await;
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
    add_worktree(&state, on_branch(branch, Some("rooted".into())))
        .await
        .unwrap();
    let checkout = root
        .path()
        .join(".dex/worktrees/api")
        .join(branch.replace('/', "-"));
    assert!(checkout.is_dir(), "the worktree was made at {checkout:?}");
    git(&checkout, &["push", "-u", "origin", branch]);
    (repo, root, bare, data, state, checkout)
}

/// Deletes the branch inside the remote, as merging a pull request does, and
/// leaves the local remote-tracking ref alone, as GitHub does.
fn merged_there(bare: &Path, branch: &str) {
    git(bare, &["update-ref", "-d", &format!("refs/heads/{branch}")]);
}

#[tokio::test]
async fn a_branch_the_remote_has_taken_is_pruned_even_though_it_looks_unmerged() {
    let (_repo, _root, bare, _data, state, checkout) =
        with_a_pushed_worktree("feat/squashed", 0).await;
    // Work of its own, so the local comparison would say "unmerged" for ever -
    // which is what a squash merge leaves behind, and what #70's rule reads.
    std::fs::write(checkout.join("work.md"), "the feature\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "the feature"]);
    git(&checkout, &["push", "origin", "feat/squashed"]);
    merged_there(bare.path(), "feat/squashed");

    let pruned = prune_worktrees(&state, automatically()).await.unwrap();

    assert_eq!(pruned.taken.len(), 1, "{pruned:?}");
    assert_eq!(pruned.taken[0].branch.as_deref(), Some("feat/squashed"));
    assert!(!checkout.exists(), "the folder went with it");
}

#[tokio::test]
async fn a_branch_the_remote_still_has_is_kept() {
    let (_repo, _root, _bare, _data, state, checkout) =
        with_a_pushed_worktree("feat/in-review", 0).await;

    let pruned = prune_worktrees(&state, automatically()).await.unwrap();

    assert!(pruned.taken.is_empty(), "{:?}", pruned.taken);
    assert_eq!(pruned.kept.len(), 1);
    assert_eq!(pruned.kept[0].because, KeptBecause::StillOpen);
    assert!(checkout.is_dir());
}

#[tokio::test]
async fn a_branch_no_remote_ever_took_is_kept() {
    // Absent from the remote because it was never pushed: the folder holds the
    // only copy, and this is the case that must never be mistaken for merged.
    let (_repo, root, _bare, _data, state, _checkout) =
        with_a_pushed_worktree("feat/pushed", 0).await;
    add_worktree(&state, on_branch("feat/private", Some("rooted".into())))
        .await
        .unwrap();
    let private = root.path().join(".dex/worktrees/api/feat-private");
    std::fs::write(private.join("secret.md"), "never pushed\n").unwrap();
    git(&private, &["add", "."]);
    git(&private, &["commit", "-m", "mine"]);

    let pruned = prune_worktrees(&state, automatically()).await.unwrap();

    let kept = pruned
        .kept
        .iter()
        .find(|kept| kept.worktree.branch.as_deref() == Some("feat/private"))
        .expect("the unpushed worktree is reported");
    assert_eq!(kept.because, KeptBecause::Unpushed);
    assert!(private.is_dir(), "and it is still there");
}

#[tokio::test]
async fn commits_made_after_the_push_keep_a_worktree_the_remote_has_taken() {
    // The remote took the branch, but there is work here it never saw.
    let (_repo, _root, bare, _data, state, checkout) =
        with_a_pushed_worktree("feat/ahead-of-origin", 0).await;
    std::fs::write(checkout.join("later.md"), "after the push\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "later"]);
    merged_there(bare.path(), "feat/ahead-of-origin");

    let pruned = prune_worktrees(&state, automatically()).await.unwrap();

    assert!(pruned.taken.is_empty(), "{:?}", pruned.taken);
    assert_eq!(pruned.kept[0].because, KeptBecause::Unpushed);
    assert!(checkout.is_dir());
}

#[tokio::test]
async fn work_within_the_grace_period_keeps_a_landed_worktree() {
    // Everything says take it, except that it was being worked in minutes ago.
    let (_repo, _root, bare, _data, state, checkout) =
        with_a_pushed_worktree("feat/just-landed", 24).await;
    merged_there(bare.path(), "feat/just-landed");

    let pruned = prune_worktrees(&state, automatically()).await.unwrap();

    assert!(pruned.taken.is_empty(), "{:?}", pruned.taken);
    assert_eq!(pruned.kept[0].because, KeptBecause::RecentlyUsed);
    assert!(checkout.is_dir(), "a day has to pass first");
}

#[tokio::test]
async fn without_a_remote_the_local_comparison_still_decides() {
    // No `origin` at all: the auto pass must still decide the old way rather
    // than keeping every worktree for ever.
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_data, state) = AppState::for_tests_with(|config| {
        config.agents.prune_after_hours = 0;
        config.agents.prune_merged_worktrees = true;
    });
    registered(&state, repo.path()).await;
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
    add_worktree(&state, on_branch("feat/done", Some("rooted".into())))
        .await
        .unwrap();
    let checkout = root.path().join(".dex/worktrees/api/feat-done");

    let pruned = prune_worktrees(&state, automatically()).await.unwrap();

    assert_eq!(pruned.taken.len(), 1, "{pruned:?}");
    assert!(!checkout.exists());
}
