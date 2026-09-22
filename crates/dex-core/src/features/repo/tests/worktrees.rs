//! Worktrees: made on a branch, found again by it, and kept apart from
//! everything outside them - inside their workspace when its root is a plain
//! folder, and at `worktree_base` when the root is inside a git checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

use dex_protocol::repo::{AddRepoArgs, AddWorktreeArgs, RemoveWorktreeArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{path_of, repo_at};
use crate::app::AppState;
use crate::features::repo::model::RepoError;
use crate::features::repo::{add, add_worktree, remove_worktree};
use crate::features::workspace;

/// Registers the repository at `dir` as `api`.
async fn registered(state: &AppState, dir: &Path) {
    add(
        state,
        AddRepoArgs {
            path: path_of(dir),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
}

/// A workspace rooted at `root`, by name. Never the default root: a bare
/// workspace's root is the real home.
async fn workspace_at(state: &AppState, root: &Path) -> String {
    workspace::create(
        state,
        CreateWorkspaceArgs {
            name: Some("rooted".into()),
            root_path: Some(path_of(root)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    "rooted".into()
}

fn on_branch(branch: &str, workspace: Option<String>) -> AddWorktreeArgs {
    AddWorktreeArgs {
        repo: "api".into(),
        branch: branch.into(),
        workspace,
    }
}

fn removing(branch: &str) -> RemoveWorktreeArgs {
    RemoveWorktreeArgs {
        repo: "api".into(),
        branch: branch.into(),
        force: false,
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git should be on PATH for these tests");
    assert!(out.status.success(), "git {args:?}: {:?}", out.stderr);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[tokio::test]
async fn a_worktree_is_created_on_a_new_branch_and_removed_again() {
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    // With no workspace, it goes to `worktree_base`, which `for_tests` puts
    // beside the temp database, never in the real home.
    let (_dir, state) = AppState::for_tests();
    registered(&state, work.path()).await;

    let made = add_worktree(&state, on_branch("fix/login", None))
        .await
        .unwrap();
    let branches: Vec<Option<&str>> = made
        .worktrees
        .iter()
        .map(|wt| wt.branch.as_deref())
        .collect();
    assert!(branches.contains(&Some("fix/login")), "{branches:?}");
    assert!(made.worktrees[0].main, "the main checkout is listed first");
    let on_disk = state.worktree_base().join("api/fix-login/README.md");
    assert!(on_disk.exists(), "the worktree has the repo's files");

    let after = remove_worktree(&state, removing("fix/login"))
        .await
        .unwrap();
    assert_eq!(after.worktrees.len(), 1, "only the main checkout is left");
}

#[tokio::test]
async fn a_worktree_on_an_existing_branch_reuses_it_rather_than_failing() {
    // An agent coming back to work it started earlier must not be blocked by
    // `-b` refusing a branch that is already there.
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, work.path()).await;

    add_worktree(&state, on_branch("fix/login", None))
        .await
        .unwrap();
    remove_worktree(&state, removing("fix/login"))
        .await
        .unwrap();

    // The branch still exists; the worktree does not.
    let again = add_worktree(&state, on_branch("fix/login", None)).await;
    assert!(again.is_ok(), "{again:?}");
    let branches: Vec<Option<String>> = again
        .unwrap()
        .worktrees
        .into_iter()
        .map(|wt| wt.branch)
        .collect();
    assert!(branches.contains(&Some("fix/login".into())), "{branches:?}");
}

#[tokio::test]
async fn a_workspace_rooted_in_a_plain_folder_keeps_its_worktrees_inside_it() {
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    let checkout = root.path().join(".dex/worktrees/api/fix-login");
    assert!(checkout.join("README.md").exists(), "inside the workspace");
    assert_eq!(
        std::fs::read_to_string(root.path().join(".dex/worktrees/.gitignore")).unwrap(),
        "*\n",
        "fenced off from git, should the root become a repository"
    );
}

#[tokio::test]
async fn a_workspace_rooted_in_a_checkout_keeps_its_worktrees_outside_it() {
    // The worktree must see nothing of the project around it: nested in a
    // checkout, everything that looks up the folder tree would reach that
    // project's files, and `git clean -ffdx` there would delete it.
    let repo = tempfile::tempdir().unwrap();
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, repo.path()).await;

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    assert!(
        state
            .worktree_base()
            .join("api/fix-login/README.md")
            .exists(),
        "at worktree_base"
    );
    assert!(
        !repo.path().join(".dex").exists(),
        "and nothing in the checkout"
    );
    assert_eq!(git(repo.path(), &["status", "--porcelain"]), "");
}

#[tokio::test]
async fn anywhere_inside_a_checkout_counts_as_inside_it() {
    // A root in a subfolder of a repository is still within its reach.
    let repo = tempfile::tempdir().unwrap();
    repo_at(repo.path());
    let docs = repo.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, &docs).await;

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    assert!(state.worktree_base().join("api/fix-login").exists());
    assert!(!docs.join(".dex").exists());
}

#[tokio::test]
async fn a_gitignore_already_in_the_worktree_folder_is_left_alone() {
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;
    let ignore = root.path().join(".dex/worktrees/.gitignore");
    std::fs::create_dir_all(ignore.parent().unwrap()).unwrap();
    std::fs::write(&ignore, "# mine\n*\n").unwrap();

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&ignore).unwrap(), "# mine\n*\n");
}

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
