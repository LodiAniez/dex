//! Worktrees: made on a branch, and kept inside the workspace they are for -
//! `<root>/.dex/worktrees/<repo>/<branch>` - with the git around that root
//! told to look away from them, twice over.

use std::path::Path;
use std::process::Command;

use dex_protocol::repo::{AddRepoArgs, AddWorktreeArgs, RemoveWorktreeArgs, ScanArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{path_of, repo_at};
use crate::app::AppState;
use crate::features::repo::{add, add_worktree, remove_worktree, scan};
use crate::features::workspace;

/// Registers the repository at `dir` as `api`.
pub(super) async fn registered(state: &AppState, dir: &Path) {
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
/// workspace's root is the real home. (The plain-root tests take the system's
/// temp folder to be in no git checkout, which is true of a stock machine.)
pub(super) async fn workspace_at(state: &AppState, root: &Path) -> String {
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

pub(super) fn on_branch(branch: &str, workspace: Option<String>) -> AddWorktreeArgs {
    AddWorktreeArgs {
        repo: "api".into(),
        branch: branch.into(),
        workspace,
    }
}

pub(super) fn removing(branch: &str) -> RemoveWorktreeArgs {
    RemoveWorktreeArgs {
        repo: "api".into(),
        branch: branch.into(),
        force: false,
    }
}

pub(super) fn git(dir: &Path, args: &[&str]) -> String {
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
async fn a_workspace_rooted_in_a_folder_of_its_own_keeps_its_worktrees_inside_it() {
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
        "fenced off from the git around it, whether or not there is any"
    );
}

#[tokio::test]
async fn a_workspace_rooted_in_a_checkout_keeps_its_worktrees_inside_it_too() {
    // The usual workspace: rooted at the repository itself. The worktree goes
    // in it all the same, and git is told to look away from it twice - in
    // `.dex/worktrees/.gitignore`, and where `git clean` cannot reach.
    let repo = tempfile::tempdir().unwrap();
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, repo.path()).await;

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    let checkout = repo.path().join(".dex/worktrees/api/fix-login");
    assert!(checkout.join("README.md").exists(), "inside the workspace");
    assert_eq!(
        git(repo.path(), &["status", "--porcelain"]),
        "",
        "and nothing new to git"
    );
    let exclude = std::fs::read_to_string(repo.path().join(".git/info/exclude")).unwrap();
    assert!(
        exclude.lines().any(|line| line == "/.dex/worktrees/"),
        "{exclude}"
    );
    assert!(
        repo.path().join(".dex/worktrees/README.txt").exists(),
        "and a note saying what is in there"
    );
}

#[tokio::test]
async fn git_cleaning_the_checkout_does_not_expose_the_worktrees_again() {
    // `git clean -fdx` deletes the `.gitignore` inside the folder - it is
    // ignored by its own rule - and skips the worktree itself as a repository
    // of its own. The `info/exclude` line is what keeps git quiet afterwards.
    let repo = tempfile::tempdir().unwrap();
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, repo.path()).await;
    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    git(repo.path(), &["clean", "-fdx"]);

    assert!(
        repo.path()
            .join(".dex/worktrees/api/fix-login/README.md")
            .exists(),
        "the worktree is a checkout of its own, which one -f leaves alone"
    );
    assert_eq!(git(repo.path(), &["status", "--porcelain"]), "");
}

#[tokio::test]
async fn a_root_in_a_subfolder_of_a_checkout_keeps_them_in_that_root() {
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

    assert!(docs.join(".dex/worktrees/api/fix-login/README.md").exists());
    // The exclude belongs to the repository holding the root, not the root.
    let exclude = std::fs::read_to_string(repo.path().join(".git/info/exclude")).unwrap();
    assert!(exclude.contains("/.dex/worktrees/"), "{exclude}");
    assert_eq!(git(repo.path(), &["status", "--porcelain"]), "");
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
async fn scanning_a_workspace_does_not_take_its_worktrees_for_repositories() {
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;
    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    let found = scan(
        &state,
        ScanArgs {
            path: path_of(root.path()),
            depth: Some(6),
        },
    )
    .await
    .unwrap();

    let paths: Vec<&str> = found.repos.iter().map(|r| r.path.as_str()).collect();
    assert!(!paths.iter().any(|p| p.contains(".dex")), "{paths:?}");
}

#[tokio::test]
async fn a_root_that_cannot_hold_worktrees_sends_them_to_worktree_base() {
    // Something already sits where the worktree folder would go, so the
    // worktree goes to `worktree_base`: further away is never wrong.
    let (repo, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(repo.path());
    std::fs::create_dir_all(root.path().join(".dex")).unwrap();
    std::fs::write(root.path().join(".dex/worktrees"), "not a folder").unwrap();
    let (_dir, state) = AppState::for_tests();
    registered(&state, repo.path()).await;
    let workspace = workspace_at(&state, root.path()).await;

    add_worktree(&state, on_branch("fix/login", Some(workspace)))
        .await
        .unwrap();

    assert!(
        state
            .worktree_base()
            .join("api/fix-login/README.md")
            .exists()
    );
}
