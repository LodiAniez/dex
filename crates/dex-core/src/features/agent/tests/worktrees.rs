//! Spawning into a worktree: the child gets a branch and a checkout of its
//! own, inside its workspace, or no spawn at all.

use dex_protocol::agent::SpawnArgs;
use dex_protocol::repo::AddRepoArgs;
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::agents;
use super::spawn::{brief, parent_agent, repo_at};
use crate::app::AppState;
use crate::features::agent::{AgentError, spawn};
use crate::features::{repo, workspace};

#[tokio::test]
async fn a_spawn_into_a_worktree_puts_the_child_on_its_own_branch() {
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    let list = workspace::create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let first = list.workspaces[0].panes[0].id.clone();
    repo::add(
        &state,
        AddRepoArgs {
            path: work.path().to_string_lossy().replace('\\', "/"),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
    parent_agent(&state, &first, "parent").await;

    let child = spawn(
        &state,
        SpawnArgs {
            repo: Some("api".into()),
            worktree: Some("fix/login".into()),
            ..brief("fix the login form", &first)
        },
    )
    .await
    .unwrap();

    assert_eq!(child.branch.as_deref(), Some("fix/login"));
    let checkout = state
        .config
        .get()
        .worktree_base
        .clone()
        .expect("for_tests sets a base")
        .join("api")
        .join("fix-login");
    assert!(checkout.join("README.md").exists(), "the worktree is real");
    assert_eq!(
        repo::branch_at(&checkout).as_deref(),
        Some("fix/login"),
        "and the child is on its own branch"
    );
}

#[tokio::test]
async fn a_worktree_that_cannot_be_made_aborts_the_whole_spawn() {
    // Falling back to the main checkout would put two agents in one working
    // tree, which is the failure worktrees exist to prevent (PRD §9.4).
    let work = tempfile::tempdir().unwrap();
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests();
    let list = workspace::create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let first = list.workspaces[0].panes[0].id.clone();
    repo::add(
        &state,
        AddRepoArgs {
            path: work.path().to_string_lossy().replace('\\', "/"),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
    parent_agent(&state, &first, "parent").await;
    let before = workspace::list(&state).await.unwrap().workspaces[0]
        .panes
        .len();

    let refused = spawn(
        &state,
        SpawnArgs {
            repo: Some("api".into()),
            // Windows cannot store this, so the worktree cannot be created.
            worktree: Some("con".into()),
            ..brief("doomed", &first)
        },
    )
    .await;

    assert!(matches!(refused, Err(AgentError::Repo(_))), "{refused:?}");
    let after = workspace::list(&state).await.unwrap().workspaces[0]
        .panes
        .len();
    assert_eq!(after, before, "no pane was left behind");
    assert_eq!(agents(&state).await.len(), 1, "and no agent row either");
}

#[tokio::test]
async fn a_spawned_agents_worktree_is_inside_its_workspace() {
    // The owner's setting, as it is: no base of their own. The workspace has
    // a root of its own, since a bare workspace's root is the real home.
    let (work, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(work.path());
    let (_dir, state) = AppState::for_tests_with(|config| config.worktree_base = None);
    let list = workspace::create(
        &state,
        CreateWorkspaceArgs {
            root_path: Some(root.path().to_string_lossy().replace('\\', "/")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let first = list.workspaces[0].panes[0].id.clone();
    repo::add(
        &state,
        AddRepoArgs {
            path: work.path().to_string_lossy().replace('\\', "/"),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
    parent_agent(&state, &first, "parent").await;

    spawn(
        &state,
        SpawnArgs {
            repo: Some("api".into()),
            worktree: Some("fix/login".into()),
            ..brief("fix the login form", &first)
        },
    )
    .await
    .unwrap();

    let checkout = root
        .path()
        .join(".dex")
        .join("worktrees")
        .join("api")
        .join("fix-login");
    assert_eq!(
        repo::branch_at(&checkout).as_deref(),
        Some("fix/login"),
        "the child's checkout is in its workspace, on its own branch"
    );
}
