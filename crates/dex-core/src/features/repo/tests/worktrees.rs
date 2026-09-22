//! Worktrees: made on a branch, found again by it, and - unless the owner
//! set a base of their own - kept inside the workspace they are for.

use std::path::{Path, PathBuf};
use std::process::Command;

use dex_protocol::repo::{AddRepoArgs, AddWorktreeArgs, RemoveWorktreeArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{path_of, repo_at};
use crate::app::AppState;
use crate::features::repo::model::RepoError;
use crate::features::repo::{add, add_worktree, remove_worktree};
use crate::features::workspace;

/// Where `for_tests` puts worktrees: beside the test database.
fn configured_base(state: &AppState) -> PathBuf {
    state
        .config
        .get()
        .worktree_base
        .clone()
        .expect("for_tests sets a base")
}

/// A state with `worktree_base` unset, as the owner's is: worktrees then go
/// inside the workspace they are for. Every test using it gives its
/// workspace a root of its own - a bare workspace's root is the real home.
fn unset_base() -> (tempfile::TempDir, AppState) {
    AppState::for_tests_with(|config| config.worktree_base = None)
}

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

/// A workspace rooted at `root`, by name.
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
    // `for_tests` puts the worktree base beside the temp database, so this
    // never writes into the real home.
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

    let made = add_worktree(
        &state,
        AddWorktreeArgs {
            repo: "api".into(),
            branch: "fix/login".into(),
            workspace: None,
        },
    )
    .await
    .unwrap();

    let branches: Vec<Option<&str>> = made
        .worktrees
        .iter()
        .map(|wt| wt.branch.as_deref())
        .collect();
    assert!(branches.contains(&Some("fix/login")), "{branches:?}");
    assert!(made.worktrees[0].main, "the main checkout is listed first");
    let on_disk = configured_base(&state)
        .join("api")
        .join("fix-login")
        .join("README.md");
    assert!(on_disk.exists(), "the worktree has the repo's files");

    let after = remove_worktree(
        &state,
        RemoveWorktreeArgs {
            repo: "api".into(),
            branch: "fix/login".into(),
            force: false,
        },
    )
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
    add(
        &state,
        AddRepoArgs {
            path: path_of(work.path()),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
    let args = || AddWorktreeArgs {
        repo: "api".into(),
        branch: "fix/login".into(),
        workspace: None,
    };

    add_worktree(&state, args()).await.unwrap();
    remove_worktree(
        &state,
        RemoveWorktreeArgs {
            repo: "api".into(),
            branch: "fix/login".into(),
            force: false,
        },
    )
    .await
    .unwrap();

    // The branch still exists; the worktree does not.
    let again = add_worktree(&state, args()).await;
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
async fn a_worktree_for_a_workspace_goes_inside_it() {
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = unset_base();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    let checkout = root.path().join(".dex/worktrees/api/fix-login");
    assert!(
        checkout.join("README.md").exists(),
        "the worktree is inside the workspace"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join(".dex/worktrees/.gitignore")).unwrap(),
        "*\n",
        "and git is told to look away from it"
    );
}

#[tokio::test]
async fn a_worktree_inside_its_own_repository_leaves_that_checkout_clean() {
    // The usual case: the workspace's root is the repository itself. Without
    // the ignore, the worktree shows there as untracked, and `git add -A`
    // takes it in as an embedded repository.
    let repo = tempfile::tempdir().unwrap();
    repo_at(repo.path());
    let (_dir, state) = unset_base();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, repo.path()).await;

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    assert!(
        repo.path()
            .join(".dex/worktrees/api/fix-login/README.md")
            .exists()
    );
    assert_eq!(
        git(repo.path(), &["status", "--porcelain"]),
        "",
        "nothing new to git"
    );
}

#[tokio::test]
async fn a_gitignore_already_in_the_worktree_directory_is_left_alone() {
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = unset_base();
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
    // Before, every worktree went under `~/dex/worktrees`; those must still
    // come out by branch now that new ones go inside their workspace.
    let (repo, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = unset_base();
    registered(&state, repo.path()).await;
    let old = elsewhere.path().join("api").join("old-place");
    git(
        repo.path(),
        &["worktree", "add", &path_of(&old), "-b", "old/place"],
    );

    let after = remove_worktree(
        &state,
        RemoveWorktreeArgs {
            repo: "api".into(),
            branch: "old/place".into(),
            force: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(after.worktrees.len(), 1, "only the main checkout is left");
    assert!(!old.exists(), "and the directory went with it");
}

#[tokio::test]
async fn removing_a_worktree_no_branch_is_on_says_so() {
    let repo = tempfile::tempdir().unwrap();
    repo_at(repo.path());
    let (_dir, state) = unset_base();
    registered(&state, repo.path()).await;

    let missing = remove_worktree(
        &state,
        RemoveWorktreeArgs {
            repo: "api".into(),
            // The main checkout's branch: never a worktree to remove.
            branch: "main".into(),
            force: false,
        },
    )
    .await;
    assert!(
        matches!(missing, Err(RepoError::NoSuchWorktree { ref branch, .. }) if branch == "main"),
        "{missing:?}"
    );
}
