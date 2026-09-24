//! Pruning worktrees against real git: what goes, what stays, and why.
//!
//! Every test here makes a real worktree and asks the real prune about it. The
//! decision table itself is unit-tested in `prune.rs`; these are about whether
//! the facts handed to it are the facts - a tree git calls dirty, a branch git
//! counts commits on, a pane with a shell running in it.

use std::path::{Path, PathBuf};

use dex_protocol::repo::{KeptBecause, PruneWorktreesArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::worktrees::{git, on_branch, registered};
use super::{path_of, repo_at};
use crate::app::AppState;
use crate::features::repo::{add_worktree, prune_worktrees};
use crate::features::workspace;
use crate::platform::pty::SpawnRequest;

fn pruning() -> PruneWorktreesArgs {
    PruneWorktreesArgs {
        repo: "api".into(),
        dry_run: false,
    }
}

/// What a spawn leaves behind: the repository, the workspace root the worktree
/// went inside, the data dir behind the state, and the worktree itself. The
/// temp dirs are returned because they delete themselves when dropped.
type Made = (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    AppState,
    PathBuf,
);

async fn with_a_worktree(branch: &str) -> Made {
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (data, state) = AppState::for_tests();
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
    (repo, root, data, state, checkout)
}

#[tokio::test]
async fn a_clean_merged_worktree_nobody_is_in_is_taken_away() {
    let (_repo, _root, _data, state, checkout) = with_a_worktree("feat/done").await;

    let pruned = prune_worktrees(&state, pruning()).await.unwrap();

    assert_eq!(pruned.taken.len(), 1, "{pruned:?}");
    assert_eq!(pruned.taken[0].branch.as_deref(), Some("feat/done"));
    assert!(pruned.kept.is_empty(), "{:?}", pruned.kept);
    assert!(!checkout.exists(), "the folder went with it");
    assert!(
        pruned.freed_bytes > 0,
        "it held something: {}",
        pruned.freed_bytes
    );
}

#[tokio::test]
async fn a_worktree_with_uncommitted_work_is_kept() {
    let (_repo, _root, _data, state, checkout) = with_a_worktree("feat/mid-flight").await;
    std::fs::write(checkout.join("notes.md"), "half an idea\n").unwrap();

    let pruned = prune_worktrees(&state, pruning()).await.unwrap();

    assert!(pruned.taken.is_empty(), "{:?}", pruned.taken);
    assert_eq!(pruned.kept.len(), 1);
    assert_eq!(pruned.kept[0].because, KeptBecause::Uncommitted);
    assert!(checkout.is_dir(), "and it is still there");
}

#[tokio::test]
async fn a_worktree_whose_branch_has_commits_of_its_own_is_kept() {
    let (_repo, _root, _data, state, checkout) = with_a_worktree("feat/ahead").await;
    std::fs::write(checkout.join("work.md"), "done and committed\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "work"]);

    let pruned = prune_worktrees(&state, pruning()).await.unwrap();

    assert_eq!(pruned.kept.len(), 1, "{pruned:?}");
    assert_eq!(pruned.kept[0].because, KeptBecause::Unmerged);
    assert!(checkout.is_dir());
}

#[tokio::test]
async fn a_worktree_a_pane_is_working_in_is_kept_however_finished_it_looks() {
    // Clean and merged, so everything else says take it - but a shell is
    // running in there, and deleting the folder under it is the one outcome
    // this command must never produce.
    let (_repo, _root, _data, state, checkout) = with_a_worktree("feat/still-going").await;
    let pane = pane_in(&state, &checkout).await;
    with_shell(&state, &pane, &checkout);

    let pruned = prune_worktrees(&state, pruning()).await.unwrap();

    assert_eq!(pruned.kept.len(), 1, "{pruned:?}");
    assert_eq!(pruned.kept[0].because, KeptBecause::InUse);
    assert!(checkout.is_dir(), "the agent's folder is still there");
    state.pty.kill(&pane).unwrap();
}

#[tokio::test]
async fn a_pane_whose_shell_has_gone_does_not_keep_its_worktree() {
    // The pane is still recorded in the worktree and nothing is running in it.
    // A record alone must not pin a worktree for ever: that is the leak.
    let (_repo, _root, _data, state, checkout) = with_a_worktree("feat/finished").await;
    let _pane = pane_in(&state, &checkout).await;

    let pruned = prune_worktrees(&state, pruning()).await.unwrap();

    assert_eq!(pruned.taken.len(), 1, "{pruned:?}");
    assert!(!checkout.exists());
}

#[tokio::test]
async fn a_dry_run_says_what_would_go_and_takes_nothing() {
    let (_repo, _root, _data, state, checkout) = with_a_worktree("feat/done").await;

    let pruned = prune_worktrees(
        &state,
        PruneWorktreesArgs {
            dry_run: true,
            ..pruning()
        },
    )
    .await
    .unwrap();

    assert!(pruned.dry_run);
    assert_eq!(pruned.taken.len(), 1);
    assert!(pruned.freed_bytes > 0);
    assert!(checkout.is_dir(), "a dry run removes nothing");

    let after = prune_worktrees(&state, pruning()).await.unwrap();
    assert_eq!(after.taken.len(), 1, "and then the real prune takes it");
    assert!(!checkout.exists());
}

#[tokio::test]
async fn the_main_checkout_is_never_taken() {
    let (repo, _root, _data, state, _checkout) = with_a_worktree("feat/done").await;

    let pruned = prune_worktrees(&state, pruning()).await.unwrap();

    assert!(pruned.taken.iter().all(|it| !it.main), "{:?}", pruned.taken);
    assert!(repo.path().join("README.md").is_file(), "left untouched");
    assert!(
        pruned.kept.iter().all(|it| !it.worktree.main),
        "nor is it reported as kept: it is not a worktree"
    );
}

/// A pane recorded in `dir`, as a spawn into a worktree leaves: the workspace
/// rooted there opens its first pane in it.
async fn pane_in(state: &AppState, dir: &Path) -> String {
    let list = workspace::create(
        state,
        CreateWorkspaceArgs {
            name: Some("working".into()),
            root_path: Some(path_of(dir)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    list.workspaces
        .iter()
        .find(|it| it.name == "working")
        .expect("the workspace just made")
        .panes
        .first()
        .expect("it opens a pane in its root")
        .id
        .clone()
}

/// Puts a real shell in the pane, running in `cwd`. Without one the pane is
/// only a row in a table, and the prune is right not to mind it.
fn with_shell(state: &AppState, pane: &str, cwd: &Path) {
    let program = if cfg!(windows) {
        PathBuf::from("cmd.exe")
    } else {
        PathBuf::from("/bin/sh")
    };
    state
        .pty
        .spawn(
            SpawnRequest {
                pane_id: pane.to_owned(),
                program,
                args: Vec::new(),
                cwd: cwd.to_path_buf(),
                env: vec![("DEX_PANE_ID".to_owned(), pane.to_owned())],
                cols: 120,
                rows: 30,
            },
            Box::new(|_| {}),
        )
        .unwrap();
}
