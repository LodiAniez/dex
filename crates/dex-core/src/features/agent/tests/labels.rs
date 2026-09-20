//! A label names an agent in the caller's workspace (issue #59).
//!
//! Labels are unique per workspace - panes carry `UNIQUE(workspace_id, label)`
//! and an agent takes its pane's - so `reviewer` may be at work in two
//! workspaces at once. The lookup behind every label target used to search
//! them all and take the newest, which is how a message, a prompt or a stop
//! could land on a stranger.

use dex_protocol::agent::{PromptAgentArgs, SpawnArgs, StopAgentArgs};
use dex_protocol::pane::LabelPaneArgs;
use dex_protocol::workspace::{CreateWorkspaceArgs, SplitDirection, SplitPaneArgs};
use serde_json::json;

use super::{fire, pane};
use crate::app::AppState;
use crate::features::agent::identity::find_target;
use crate::features::agent::model::{Agent, AgentError};
use crate::features::agent::{prompt, spawn, stop};
use crate::features::workspace;

/// Another workspace with a pane of its own: its root, its id, and that
/// pane's id. Keep the directory alive for the whole test.
async fn another_workspace(state: &AppState) -> (tempfile::TempDir, String, String) {
    let root = tempfile::tempdir().unwrap();
    let made = workspace::create(
        state,
        CreateWorkspaceArgs {
            name: Some("elsewhere".into()),
            root_path: Some(root.path().to_string_lossy().replace('\\', "/")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let workspace = made
        .workspaces
        .into_iter()
        .find(|ws| ws.name == "elsewhere")
        .expect("the second workspace");
    let pane = workspace.panes[0].id.clone();
    (root, workspace.id, pane)
}

/// The workspace a pane belongs to.
async fn workspace_of(state: &AppState, pane: &str) -> String {
    let pane = pane.to_owned();
    state
        .db
        .call(move |conn| workspace::find_pane_workspace(conn, &pane))
        .await
        .unwrap()
        .expect("the pane's workspace")
}

/// Spawns an agent labelled `label` beside `from_pane`, and returns its id.
async fn labelled(state: &AppState, from_pane: &str, label: &str) -> String {
    spawn(
        state,
        SpawnArgs {
            task: format!("be the {label}"),
            repo: None,
            worktree: None,
            label: Some(label.to_owned()),
            direction: None,
            pane: Some(from_pane.to_owned()),
            workspace: None,
        },
    )
    .await
    .unwrap()
    .agent
}

/// A pane labelled `label` with Claude Code started in it: an agent whose
/// label is its pane's, not its own.
async fn labelled_pane(state: &AppState, beside: &str, label: &str) -> String {
    let made = workspace::split_pane(
        state,
        SplitPaneArgs {
            pane: beside.to_owned(),
            direction: SplitDirection::Right,
            cwd: None,
            label: Some(label.to_owned()),
            kind: None,
        },
    )
    .await
    .unwrap();
    // In this workspace: the other one may have a pane of the same name.
    let pane = made
        .workspaces
        .iter()
        .find(|ws| ws.panes.iter().any(|pane| pane.id == beside))
        .expect("the workspace split")
        .panes
        .iter()
        .find(|pane| pane.label.as_deref() == Some(label))
        .expect("the labelled pane")
        .id
        .clone();
    // Its own session: two Claude Codes in two panes are two agents.
    fire(
        state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": format!("s-{pane}"), "source": "startup" }),
    )
    .await;
    pane
}

/// What `target` names to a caller in `within`.
async fn found(state: &AppState, target: &str, within: Option<&str>) -> Result<Agent, AgentError> {
    let (target, within) = (target.to_owned(), within.map(str::to_owned));
    state
        .db
        .call(move |conn| find_target(conn, &target, within.as_deref()))
        .await
        .unwrap()
}

/// The pane an agent runs in.
async fn pane_of(state: &AppState, agent: &str) -> String {
    let agent = agent.to_owned();
    state
        .db
        .call(move |conn| crate::features::agent::store::find_agent(conn, &agent))
        .await
        .unwrap()
        .expect("the agent")
        .pane_id
        .expect("its pane")
}

/// Claude Code starts in the agent's pane, so it can be prompted.
async fn started(state: &AppState, agent: &str) {
    let pane = pane_of(state, agent).await;
    fire(
        state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": format!("s-{pane}"), "source": "startup" }),
    )
    .await;
}

fn prompt_args(
    target: &str,
    from_pane: Option<String>,
    workspace: Option<String>,
) -> PromptAgentArgs {
    PromptAgentArgs {
        agent: target.to_owned(),
        text: "carry on".to_owned(),
        from_pane,
        workspace,
    }
}

#[tokio::test]
async fn a_label_names_the_agent_in_the_callers_own_workspace() {
    let (_dir, state, here) = pane().await;
    let (_there_root, there_id, there) = another_workspace(&state).await;
    let mine = labelled(&state, &here, "reviewer").await;
    // Theirs is the newest, which is the one the old lookup would have taken.
    let theirs = labelled(&state, &there, "reviewer").await;
    assert_ne!(mine, theirs);

    let here_id = workspace_of(&state, &here).await;
    assert_eq!(
        found(&state, "reviewer", Some(&here_id)).await.unwrap().id,
        mine
    );
    assert_eq!(
        found(&state, "reviewer", Some(&there_id)).await.unwrap().id,
        theirs
    );
}

#[tokio::test]
async fn a_label_at_work_only_in_another_workspace_names_nobody() {
    let (_dir, state, here) = pane().await;
    let (_there_root, _there_id, there) = another_workspace(&state).await;
    labelled(&state, &there, "reviewer").await;

    let here_id = workspace_of(&state, &here).await;
    let missing = found(&state, "reviewer", Some(&here_id)).await.unwrap_err();
    assert!(
        matches!(missing, AgentError::NoSuchAgent(ref name) if name == "reviewer"),
        "{missing:?}"
    );
}

#[tokio::test]
async fn a_pane_label_is_the_callers_workspaces_pane() {
    // The label lives on the pane, not on the agent: the other half of how an
    // agent is named. Two workspaces with a `backend` pane used to be an
    // ambiguous target, even to a caller who was standing in one of them.
    let (_dir, state, here) = pane().await;
    let (_there_root, there_id, there) = another_workspace(&state).await;
    let mine = labelled_pane(&state, &here, "backend").await;
    let theirs = labelled_pane(&state, &there, "backend").await;

    let here_id = workspace_of(&state, &here).await;
    assert_eq!(
        found(&state, "backend", Some(&here_id))
            .await
            .unwrap()
            .pane_id,
        Some(mine)
    );
    assert_eq!(
        found(&state, "backend", Some(&there_id))
            .await
            .unwrap()
            .pane_id,
        Some(theirs)
    );
}

#[tokio::test]
async fn an_id_names_its_agent_in_whichever_workspace_it_is() {
    // Ids are unique everywhere, and asking for one is asking for that agent.
    let (_dir, state, here) = pane().await;
    let (_there_root, _there_id, there) = another_workspace(&state).await;
    let theirs = labelled(&state, &there, "reviewer").await;

    let here_id = workspace_of(&state, &here).await;
    assert_eq!(
        found(&state, &theirs, Some(&here_id)).await.unwrap().id,
        theirs
    );
}

#[tokio::test]
async fn with_no_workspace_to_go_on_a_label_in_two_of_them_is_refused() {
    // `dex agent stop reviewer` from outside any pane: Dex will not guess.
    let (_dir, state, here) = pane().await;
    let (_there_root, _there_id, there) = another_workspace(&state).await;
    let alone = labelled(&state, &here, "writer").await;
    let mine = labelled(&state, &here, "reviewer").await;
    labelled(&state, &there, "reviewer").await;

    let refused = found(&state, "reviewer", None).await.unwrap_err();
    match refused {
        AgentError::AmbiguousTarget { target, candidates } => {
            assert_eq!(target, "reviewer");
            assert_eq!(candidates.len(), 2, "{candidates:?}");
            assert!(candidates.iter().any(|one| one.contains("elsewhere")));
            // Pasted back as a target, a candidate has to be one: whole ids.
            assert!(
                candidates.iter().any(|one| one.contains(&mine)),
                "{candidates:?} should name {mine}"
            );
        }
        other => panic!("{other:?}"),
    }
    // One match anywhere is still an answer: the common case is one workspace.
    assert_eq!(found(&state, "writer", None).await.unwrap().id, alone);
}

#[tokio::test]
async fn stopping_by_label_stops_the_one_in_the_callers_workspace() {
    let (_dir, state, here) = pane().await;
    let (_there_root, there_id, there) = another_workspace(&state).await;
    let mine = labelled(&state, &here, "reviewer").await;
    let theirs = labelled(&state, &there, "reviewer").await;

    let stopped = stop(
        &state,
        StopAgentArgs {
            agent: "reviewer".to_owned(),
            graceful: false,
            close_pane: false,
            from_pane: Some(here.clone()),
            workspace: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(stopped.agent, mine);

    // And theirs is still at work, named from their own workspace.
    assert_eq!(
        found(&state, "reviewer", Some(&there_id)).await.unwrap().id,
        theirs
    );
}

#[tokio::test]
async fn two_agents_sharing_a_label_in_one_workspace_resolve_to_the_newest() {
    // An agent keeps the label it was hired with, so relabelling its pane and
    // hiring again leaves two `reviewer`s in one workspace. Old data, not an
    // ambiguous request: naming the workspace could not narrow it, and Dex
    // has always taken the newest.
    let (_dir, state, here) = pane().await;
    let first = labelled(&state, &here, "reviewer").await;
    let its_pane = pane_of(&state, &first).await;
    workspace::label_pane(
        &state,
        LabelPaneArgs {
            pane: its_pane,
            label: Some("reviewer-was".into()),
        },
    )
    .await
    .unwrap();
    let second = labelled(&state, &here, "reviewer").await;

    let here_id = workspace_of(&state, &here).await;
    assert_eq!(
        found(&state, "reviewer", Some(&here_id)).await.unwrap().id,
        second,
        "the newest, not an error the caller cannot act on"
    );
}

#[tokio::test]
async fn prompting_by_label_types_at_the_agent_in_the_callers_workspace() {
    // Only ours has Claude Code started, so which one was resolved shows in
    // how the prompt fails: ours gets as far as its pane (which has no shell
    // in tests), theirs is refused before that.
    let (_dir, state, here) = pane().await;
    let (_there_root, there_id, there) = another_workspace(&state).await;
    let mine = labelled(&state, &here, "reviewer").await;
    labelled(&state, &there, "reviewer").await;
    started(&state, &mine).await;

    let ours = prompt(&state, prompt_args("reviewer", Some(here.clone()), None)).await;
    assert!(
        matches!(ours, Err(AgentError::Target(_))),
        "ours was reached, and its pane has no shell: {ours:?}"
    );

    let theirs = prompt(&state, prompt_args("reviewer", None, Some(there_id))).await;
    assert!(
        matches!(theirs, Err(AgentError::NotPromptable(_))),
        "theirs has not started Claude Code: {theirs:?}"
    );
}

#[tokio::test]
async fn a_workspace_that_does_not_exist_is_said_so_rather_than_ignored() {
    let (_dir, state, here) = pane().await;
    let mine = labelled(&state, &here, "reviewer").await;

    let stopped = stop(
        &state,
        StopAgentArgs {
            agent: mine,
            graceful: false,
            close_pane: false,
            from_pane: Some(here),
            workspace: Some("no-such-workspace".to_owned()),
        },
    )
    .await;
    assert!(matches!(stopped, Err(AgentError::Target(_))), "{stopped:?}");
}
