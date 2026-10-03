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

use std::path::Path;

use dex_protocol::repo::PruneWorktreesArgs;

use crate::app::AppState;
use crate::features::{context, repo};
use crate::platform::{clock, wsl};

/// At most one pass an hour. The watchdog sweeps every fifteen seconds, and
/// each pass asks every repository's remote: that is not a question to ask four
/// times a minute.
const EVERY_MS: i64 = 60 * 60 * 1_000;

/// This pass's name in `AppState::claim_slot`.
const SLOT: &str = "prune-landed-worktrees";

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
    if !state.claim_slot(SLOT, clock::now_millis(), EVERY_MS) {
        return;
    }
    let repos = match repo::list(state).await {
        Ok(listed) => listed.repos,
        Err(err) => {
            tracing::warn!(%err, "could not list the repositories to prune");
            return;
        }
    };
    // Asked once, and only if a repository is registered on a distro's side.
    let distros_up = repos
        .iter()
        .any(|repo| repo::distro_of(Path::new(&repo.path)).is_some())
        .then(wsl::running_distros);
    for registered in repos {
        if asleep(&registered.path, distros_up.as_deref()) {
            tracing::debug!(name = %registered.name, "its distro is not running; left alone");
            continue;
        }
        prune_one(state, &registered.id, &registered.name, &registered.path).await;
    }
}

/// Whether a repository is on a distro that is not running.
///
/// #77 guarded the worktrees and not the repository they came from, and the
/// repository this was written for is registered at
/// `//wsl.localhost/Ubuntu/...`, so an hourly pass would still have reached
/// into a distro's side, and reading one starts it (review).
///
/// A repository nobody can reach is simply skipped. The owner running the
/// prune by hand has asked for it and may wake whatever it takes; a pass on a
/// timer may not.
fn asleep(path: &str, distros_up: Option<&[String]>) -> bool {
    let Some(distro) = repo::distro_of(Path::new(path)) else {
        return false;
    };
    !distros_up
        .unwrap_or_default()
        .iter()
        .any(|up| up.eq_ignore_ascii_case(&distro))
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
    // The size is said because it was measured: the prune walks every file of
    // every worktree it takes, and before this it filled a number nothing read
    // (review). It is what the files added up to, not what the disk gives
    // back - `repo::size` says why.
    let said = format!(
        "Dex took away {} worktree{} of {name} whose work had landed, {} of files: {}.",
        branches.len(),
        if branches.len() == 1 { "" } else { "s" },
        held(pruned.taken_bytes),
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

/// One line in a workspace's log, and in `.dex/activity.log` with it: a
/// removal has to be findable by whoever greps the folder afterwards, not only
/// in the office (review).
///
/// Best effort: the worktrees are already gone, and a line that could not be
/// written is not worth failing anything over - though it is worth a warning,
/// because a removal nobody can account for is worse than the disk it saved.
async fn say(state: &AppState, workspace_id: &str, body: &str) {
    let now = clock::now_millis();
    let written =
        context::record_and_mirror(state, workspace_id, "note", body.to_owned(), now).await;
    if let Err(err) = written {
        tracing::warn!(%err, "took a worktree away but could not say so in the log");
    }
}

/// Bytes as a line in a log reads them. Rough on purpose: this is a sentence
/// about a folder that has gone, not a measurement anyone will subtract.
fn held(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut size = bytes as f64 / 1024.0;
    if size < 1.0 {
        return format!("{bytes} B");
    }
    for unit in UNITS {
        if size < 1024.0 || unit == "TB" {
            return format!("{size:.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{bytes} B")
}

#[cfg(test)]
mod tests {
    use super::{asleep, held};

    #[test]
    fn a_repository_on_a_distro_that_is_not_running_is_left_alone() {
        let on_ubuntu = "//wsl.localhost/Ubuntu/home/me/patly";
        assert!(asleep(on_ubuntu, Some(&["Debian".to_owned()])));
        assert!(asleep(on_ubuntu, Some(&[])));
        assert!(asleep(on_ubuntu, None));
        assert!(!asleep(on_ubuntu, Some(&["ubuntu".to_owned()])), "it is up");
    }

    #[test]
    fn a_repository_on_windows_is_never_asleep() {
        assert!(!asleep("C:/code/api", Some(&[])));
        assert!(!asleep("C:/code/api", None));
    }

    #[test]
    fn what_went_is_said_in_the_largest_unit_that_fits() {
        assert_eq!(held(0), "0 B");
        assert_eq!(held(900), "900 B");
        assert_eq!(held(4096), "4.0 KB");
        assert_eq!(held(3_221_225_472), "3.0 GB");
    }
}
