//! Spawning into a worktree: the child gets a branch and a checkout of its
//! own, kept apart from everything outside it, or no spawn at all.

use std::path::Path;

use dex_protocol::agent::SpawnArgs;
use dex_protocol::repo::AddRepoArgs;
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::agents;
use super::spawn::{brief, parent_agent, repo_at};
use crate::app::AppState;
use crate::features::agent::{AgentError, spawn};
use crate::features::{repo, workspace};

fn path_of(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

/// A workspace rooted at `root` with a parent agent in its first pane, and
/// the repository at `work` registered as `api`. Returns that first pane.
/// Never the default root: a bare workspace's root is the real home.
async fn lead_in(state: &AppState, root: &Path, work: &Path) -> String {
    let list = workspace::create(
        state,
        CreateWorkspaceArgs {
            root_path: Some(path_of(root)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let first = list.workspaces[0].panes[0].id.clone();
    repo::add(
        state,
        AddRepoArgs {
            path: path_of(work),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
    parent_agent(state, &first, "parent").await;
    first
}

fn into_worktree(branch: &str, task: &str, from: &str) -> SpawnArgs {
    SpawnArgs {
        repo: Some("api".into()),
        worktree: Some(branch.into()),
        ..brief(task, from)
    }
}

#[tokio::test]
async fn a_spawned_agents_worktree_is_inside_its_workspace() {
    let (work, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    let first = lead_in(&state, root.path(), work.path()).await;

    let child = spawn(
        &state,
        into_worktree("fix/login", "fix the login form", &first),
    )
    .await
    .unwrap();

    assert_eq!(child.branch.as_deref(), Some("fix/login"));
    let checkout = root.path().join(".dex/worktrees/api/fix-login");
    assert_eq!(
        repo::branch_at(&checkout).as_deref(),
        Some("fix/login"),
        "the child's checkout is in its workspace, on its own branch"
    );
}

#[tokio::test]
async fn a_spawned_agents_worktree_is_inside_the_workspace_even_when_that_is_the_repo() {
    // The usual workspace: rooted at the repository itself.
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    let first = lead_in(&state, work.path(), work.path()).await;

    spawn(
        &state,
        into_worktree("fix/login", "fix the login form", &first),
    )
    .await
    .unwrap();

    let checkout = work.path().join(".dex/worktrees/api/fix-login");
    assert_eq!(repo::branch_at(&checkout).as_deref(), Some("fix/login"));
    assert!(
        !state.worktree_base().join("api").exists(),
        "not outside it"
    );
}

#[tokio::test]
async fn a_worktree_that_cannot_be_made_aborts_the_whole_spawn() {
    // Falling back to the main checkout would put two agents in one working
    // tree, which is the failure worktrees exist to prevent (PRD §9.4).
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    let first = lead_in(&state, work.path(), work.path()).await;
    let before = workspace::list(&state).await.unwrap().workspaces[0]
        .panes
        .len();

    // Windows cannot store this, so the worktree cannot be created.
    let refused = spawn(&state, into_worktree("con", "doomed", &first)).await;

    assert!(matches!(refused, Err(AgentError::Repo(_))), "{refused:?}");
    let after = workspace::list(&state).await.unwrap().workspaces[0]
        .panes
        .len();
    assert_eq!(after, before, "no pane was left behind");
    assert_eq!(agents(&state).await.len(), 1, "and no agent row either");
}
