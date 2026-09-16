//! `agent.spawn` (PRD §9.4): the six-step sequence and, above all, the
//! guardrails. A skill that can spawn agents can spawn them recursively, so
//! the limits are tested as carefully as the happy path.

use std::process::Command;

use dex_protocol::agent::{AgentEventArgs, SpawnArgs};
use dex_protocol::context::{Caller, DigestArgs};
use dex_protocol::repo::AddRepoArgs;
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::{agents, pane};
use crate::app::AppState;
use crate::features::agent::{AgentError, event, spawn};
use crate::features::{context, repo, workspace};

fn brief(task: &str, pane: &str) -> SpawnArgs {
    SpawnArgs {
        task: task.into(),
        repo: None,
        worktree: None,
        label: None,
        direction: None,
        pane: Some(pane.into()),
        workspace: None,
    }
}

/// A git repository with one commit.
fn repo_at(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git should be on PATH for these tests");
        assert!(out.status.success(), "git {args:?}: {:?}", out.stderr);
    };
    run(&["init", "--initial-branch=main"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-m", "first"]);
}

/// Registers a live parent agent in a pane, as a `SessionStart` hook would.
async fn parent_agent(state: &AppState, pane: &str, session: &str) {
    event(
        state,
        AgentEventArgs {
            kind: "session-start".into(),
            pane: pane.into(),
            agent: None,
            stamp: 1,
            input: serde_json::json!({ "session_id": session, "source": "startup" }),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_spawned_agent_gets_its_own_pane_with_its_parent_and_brief() {
    let (_dir, state, first) = pane().await;
    parent_agent(&state, &first, "parent").await;

    let child = spawn(&state, brief("port the auth module", &first))
        .await
        .unwrap();

    assert_ne!(child.pane, first, "it gets a pane of its own");
    let all = agents(&state).await;
    let row = all.iter().find(|agent| agent.id == child.agent).unwrap();
    assert_eq!(row.task_brief.as_deref(), Some("port the auth module"));
    assert_eq!(
        row.permission_mode.as_deref(),
        Some("auto"),
        "it must not stop for approval, and the header shows the mode"
    );
    assert_eq!(row.pane_id.as_deref(), Some(child.pane.as_str()));
}

#[tokio::test]
async fn the_childs_opening_digest_names_its_parent_and_its_brief() {
    // The M7 acceptance criterion, and the reason the brief never goes through
    // a shell: the child learns what it is for by reading its own context.
    let (_dir, state, first) = pane().await;
    parent_agent(&state, &first, "parent").await;
    let child = spawn(&state, brief("port the auth module", &first))
        .await
        .unwrap();

    let digest = context::digest(
        &state,
        DigestArgs {
            kind: "full".into(),
            rate_limited: false,
            max_chars: None,
            caller: Caller {
                pane: Some(child.pane.clone()),
                agent: Some(child.agent.clone()),
                ..Default::default()
            },
        },
    )
    .await
    .unwrap()
    .text
    .expect("a spawned agent is always told something");

    assert!(
        digest.contains("port the auth module"),
        "the brief reaches the child: {digest}"
    );
    assert!(
        digest.contains("This agent's task"),
        "stated as a fact, not an instruction: {digest}"
    );
}

#[tokio::test]
async fn the_child_adopts_its_row_when_its_session_starts() {
    // Without this, SessionStart would register a second agent and orphan the
    // brief and parent that `spawn` recorded.
    let (_dir, state, first) = pane().await;
    parent_agent(&state, &first, "parent").await;
    let child = spawn(&state, brief("do the thing", &first)).await.unwrap();

    // The child's Claude Code starts, with no DEX_AGENT_ID reaching it.
    event(
        &state,
        AgentEventArgs {
            kind: "session-start".into(),
            pane: child.pane.clone(),
            agent: None,
            stamp: 5,
            input: serde_json::json!({ "session_id": "child", "source": "startup" }),
        },
    )
    .await
    .unwrap();

    let in_pane: Vec<_> = agents(&state)
        .await
        .into_iter()
        .filter(|agent| agent.pane_id.as_deref() == Some(child.pane.as_str()))
        .collect();
    assert_eq!(in_pane.len(), 1, "one agent, not two: {in_pane:?}");
    assert_eq!(in_pane[0].id, child.agent);
    assert_eq!(in_pane[0].task_brief.as_deref(), Some("do the thing"));
}

#[tokio::test]
async fn spawning_deeper_than_the_limit_is_refused_with_a_way_forward() {
    let (_dir, state, first) = pane().await;
    parent_agent(&state, &first, "parent").await;

    // Depth 1.
    let child = spawn(&state, brief("first level", &first)).await.unwrap();
    event(
        &state,
        AgentEventArgs {
            kind: "session-start".into(),
            pane: child.pane.clone(),
            agent: None,
            stamp: 5,
            input: serde_json::json!({ "session_id": "child", "source": "startup" }),
        },
    )
    .await
    .unwrap();

    // Depth 2 is allowed; depth 3 is not.
    let grandchild = spawn(&state, brief("second level", &child.pane))
        .await
        .unwrap();
    event(
        &state,
        AgentEventArgs {
            kind: "session-start".into(),
            pane: grandchild.pane.clone(),
            agent: None,
            stamp: 6,
            input: serde_json::json!({ "session_id": "grandchild", "source": "startup" }),
        },
    )
    .await
    .unwrap();

    let refused = spawn(&state, brief("third level", &grandchild.pane)).await;
    assert!(
        matches!(refused, Err(AgentError::DepthLimit { depth: 3, max: 2 })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn spawning_past_the_concurrency_limit_is_refused() {
    let (_dir, state, first) = pane().await;
    parent_agent(&state, &first, "parent").await;

    // The parent counts, so five more reach the limit of six.
    for i in 0..5 {
        spawn(&state, brief(&format!("job {i}"), &first))
            .await
            .unwrap_or_else(|err| panic!("spawn {i} should be allowed: {err:?}"));
    }
    let refused = spawn(&state, brief("one too many", &first)).await;
    assert!(
        matches!(
            refused,
            Err(AgentError::ConcurrencyLimit { live: 6, max: 6 })
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_spawn_with_nothing_to_do_or_no_repo_to_branch_is_refused() {
    let (_dir, state, first) = pane().await;
    assert!(matches!(
        spawn(&state, brief("   ", &first)).await,
        Err(AgentError::EmptyBrief)
    ));

    let orphan_worktree = SpawnArgs {
        worktree: Some("fix/login".into()),
        ..brief("something", &first)
    };
    assert!(matches!(
        spawn(&state, orphan_worktree).await,
        Err(AgentError::WorktreeWithoutRepo)
    ));
}

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
    let checkout = state.worktree_base().join("api").join("fix-login");
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
