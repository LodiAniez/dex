//! Taking an agent's worktree away once its work has landed (issue #74).
//!
//! The owner should not have to run a prune. An agent finishes, its branch is
//! merged, and the checkout sits there with its dependencies installed until
//! the disk fills - which is how #70 was found in the first place.
//!
//! This pass lives in the agent slice rather than the repo slice for two
//! reasons. The repo slice cannot reach the context slice to write the
//! workspace log, because context already calls repo and a cycle between
//! slices is not allowed (conventions 1.2). And what a finished agent leaves
//! behind is this slice's business: repo owns the mechanism, this owns the
//! policy of when to use it.
//!
//! Everything about what may and may not be taken is `repo::verdict`, reached
//! through `worktree.prune` with `auto` set, so the owner can watch the same
//! pass by hand with `dex worktree prune <repo> --auto` before trusting it.

use dex_protocol::repo::PruneWorktreesArgs;

use crate::app::AppState;
use crate::features::{context, repo};
use crate::platform::clock;

/// At most one pass an hour. The watchdog sweeps every fifteen seconds, and
/// each pass asks every repository's remote: that is not a question to ask four
/// times a minute.
const EVERY_MS: i64 = 60 * 60 * 1_000;

/// Takes away the worktrees whose work has landed, if the owner asked for that
/// and an hour has passed since the last pass.
///
/// Nothing here fails the sweep it is part of: a repository that cannot be
/// reached, or a worktree git will not remove, is a warning and the next
/// repository is tried.
pub async fn prune_landed(state: &AppState) {
    if !state.config.get().agents.prune_merged_worktrees {
        return;
    }
    if !state.claim_slot(clock::now_millis(), EVERY_MS) {
        return;
    }
    let repos = match repo::list(state).await {
        Ok(listed) => listed.repos,
        Err(err) => {
            tracing::warn!(%err, "could not list the repositories to prune");
            return;
        }
    };
    for registered in repos {
        prune_one(state, &registered.id, &registered.name, &registered.path).await;
    }
}

/// One repository's worktrees, and a line in the log of every workspace that
/// was using it.
async fn prune_one(state: &AppState, id: &str, name: &str, path: &str) {
    // Asked before the prune: it forgets the rows these come from.
    let watching = workspaces_using(state, id).await;
    let pruned = repo::prune_worktrees(
        state,
        PruneWorktreesArgs {
            repo: path.to_owned(),
            dry_run: false,
            auto: true,
        },
    )
    .await;
    let pruned = match pruned {
        Ok(pruned) => pruned,
        Err(err) => {
            tracing::warn!(%name, %err, "could not prune the worktrees of a repository");
            return;
        }
    };
    if pruned.taken.is_empty() {
        return;
    }
    let branches: Vec<String> = pruned
        .taken
        .iter()
        .filter_map(|worktree| worktree.branch.clone())
        .collect();
    tracing::info!(%name, taken = branches.len(), "took away worktrees whose work had landed");
    let said = format!(
        "Dex took away {} worktree{} of {name} whose work had landed: {}.",
        branches.len(),
        if branches.len() == 1 { "" } else { "s" },
        branches.join(", ")
    );
    for workspace in watching {
        say(state, &workspace, &said).await;
    }
    state.bus.publish("repos");
    state.bus.publish("context");
}

/// The workspaces that recorded using this repository, so each one's activity
/// pane says what happened rather than a worktree simply vanishing.
async fn workspaces_using(state: &AppState, repo_id: &str) -> Vec<String> {
    let repo_id = repo_id.to_owned();
    match state
        .db
        .call(move |conn| repo::workspaces_using(conn, &repo_id))
        .await
    {
        Ok(workspaces) => workspaces,
        Err(err) => {
            tracing::warn!(%err, "could not find which workspaces used a repository");
            Vec::new()
        }
    }
}

/// One line in a workspace's log. Best effort: the worktrees are already gone,
/// and a line that could not be written is not worth failing anything over -
/// though it is worth a warning, because a removal nobody can account for
/// afterwards is worse than the disk it saved.
async fn say(state: &AppState, workspace_id: &str, body: &str) {
    let (workspace_id, body) = (workspace_id.to_owned(), body.to_owned());
    let now = clock::now_millis();
    let written = state
        .db
        .call(move |conn| context::record_event(conn, &workspace_id, None, "note", body, now))
        .await;
    if let Err(err) = written {
        tracing::warn!(%err, "took a worktree away but could not say so in the log");
    }
}
