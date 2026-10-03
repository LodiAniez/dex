//! The hourly pass that takes a finished agent's worktree away (issue #74):
//! that it is off until the owner turns it on, that it says what it did, and
//! that it runs occasionally rather than at the sweep's own pace.

use std::path::Path;

use dex_protocol::context::{Caller, ScopeArgs};
use dex_protocol::repo::{AddRepoArgs, AddWorktreeArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::super::pruning;
use super::spawn::repo_at;
use crate::app::AppState;
use crate::features::{context, repo, workspace};

fn path_of(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

/// A workspace with a registered repository and one worktree in it that
/// nothing speaks for: clean, nobody in it, nothing of its own to commit, and
/// no grace period left.
type Ready = (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    AppState,
    String,
);

async fn ready_to_prune(on: bool) -> Ready {
    let (work, root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    repo_at(work.path());
    let (data, state) = AppState::for_tests_with(|config| {
        config.agents.prune_merged_worktrees = on;
        config.agents.prune_after_hours = 0;
    });
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
    repo::add(
        &state,
        AddRepoArgs {
            path: path_of(work.path()),
            name: Some("api".into()),
        },
    )
    .await
    .unwrap();
    add(&state, "feat/done").await;
    let at = path_of(root.path());
    (work, root, data, state, at)
}

async fn add(state: &AppState, branch: &str) {
    repo::add_worktree(
        state,
        AddWorktreeArgs {
            repo: "api".into(),
            branch: branch.to_owned(),
            workspace: Some("rooted".into()),
        },
    )
    .await
    .unwrap();
}

fn worktree(root: &str, branch: &str) -> std::path::PathBuf {
    Path::new(root)
        .join(".dex/worktrees/api")
        .join(branch.replace('/', "-"))
}

#[tokio::test]
async fn nothing_happens_until_the_owner_turns_it_on() {
    // The default. Deleting a checkout is not a thing to start doing to
    // somebody's machine because they installed an update.
    let (_work, _root, _data, state, root) = ready_to_prune(false).await;
    let checkout = worktree(&root, "feat/done");
    assert!(checkout.is_dir());

    pruning::prune_landed(&state).await;

    assert!(checkout.is_dir(), "it is off, so nothing was taken");
}

#[tokio::test]
async fn a_worktree_whose_work_has_landed_goes_and_is_written_in_the_log() {
    let (_work, _root, _data, state, root) = ready_to_prune(true).await;
    let checkout = worktree(&root, "feat/done");

    pruning::prune_landed(&state).await;

    assert!(!checkout.exists(), "the worktree went");
    let events = context::events(
        &state,
        ScopeArgs::for_caller(Caller {
            workspace: Some("rooted".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let said = events
        .events
        .iter()
        .any(|event| event.body.contains("feat/done") && event.body.contains("took away"));
    assert!(
        said,
        "a worktree never simply vanishes: {:?}",
        events.events
    );

    // And in the file, which is where someone greps when they go looking in
    // the folder rather than opening the office (review).
    let log = std::fs::read_to_string(Path::new(&root).join(".dex/activity.log"))
        .expect("the workspace's activity log");
    assert!(
        log.contains("feat/done") && log.contains("took away"),
        "the mirror has it too: {log}"
    );
}

#[tokio::test]
async fn the_pass_runs_occasionally_rather_than_every_sweep() {
    // The sweep calls this every fifteen seconds, and every pass asks each
    // repository's remote. A second worktree made right after a pass is left
    // for the next hour.
    let (_work, _root, _data, state, root) = ready_to_prune(true).await;

    pruning::prune_landed(&state).await;
    assert!(!worktree(&root, "feat/done").exists());

    add(&state, "feat/second").await;
    pruning::prune_landed(&state).await;

    assert!(
        worktree(&root, "feat/second").is_dir(),
        "the hour has not passed, so the second pass did nothing"
    );
}
