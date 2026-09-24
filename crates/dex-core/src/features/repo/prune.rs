//! `worktree.prune`: taking away the worktrees nobody is working in any more
//! (issue #70).
//!
//! An agent gets a worktree and nothing ever took it away again, so a machine
//! running agents for a few weeks ends up with a checkout per branch, each one
//! with its dependencies installed. This is the sweep that clears them.
//!
//! It is deliberately timid. A worktree may hold the only copy of an
//! afternoon's work, and losing that is far worse than keeping a few gigabytes,
//! so every question that cannot be answered is answered "keep it": a tree git
//! would not report on is dirty, a branch git would not compare is unmerged,
//! and anything a running shell is sitting in stays whatever else is true.
//! What was kept, and why, is reported rather than left silent.

use std::path::Path;

use dex_protocol::repo::{KeptBecause, KeptWorktree, PruneWorktreesArgs, Pruned, WorktreeView};

use super::commands::resolve;
use super::model::RepoError;
use super::worktree_commands::take_away;
use super::{logic, placement, size, store};
use crate::app::AppState;
use crate::features::workspace;
use crate::platform::paths;
use crate::platform::proc::{self, GitError};

/// `worktree.prune`: removes every worktree of a repo that is safe to remove,
/// and says what was left behind and why.
pub async fn prune(state: &AppState, args: PruneWorktreesArgs) -> Result<Pruned, RepoError> {
    let repo = resolve(state, &args.repo).await?;
    let busy = busy_folders(state).await?;
    let (repo_path, dry_run) = (repo.path.clone(), args.dry_run);
    let pruned = tokio::task::spawn_blocking(move || sweep(Path::new(&repo_path), &busy, dry_run))
        .await
        .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))??;
    if !pruned.dry_run {
        forget(state, &repo.id, &pruned.taken).await?;
    }
    Ok(pruned)
}

/// The folders of panes whose shell is running.
///
/// The pane rather than the agent: an agent that ended may have left a shell
/// behind in the worktree, and a pane the owner opened there themselves has no
/// agent at all. Either way something is in there, and the PTY supervisor is
/// the one that knows for certain.
async fn busy_folders(state: &AppState) -> Result<Vec<String>, RepoError> {
    let panes = state
        .db
        .call(|conn| workspace::terminal_panes(conn))
        .await?;
    Ok(panes
        .into_iter()
        .filter(|(pane, _, _)| state.pty.shell_pid(pane).is_some())
        .map(|(_, cwd, _)| cwd)
        .collect())
}

/// Judges every worktree of the repository, main checkout aside, and takes
/// away the ones nothing speaks for.
fn sweep(repo: &Path, busy: &[String], dry_run: bool) -> Result<Pruned, RepoError> {
    let listed = proc::git(repo, &["worktree", "list", "--porcelain"])?;
    let mut checkouts = logic::parse_worktrees(&listed.stdout).into_iter();
    // Git lists the main checkout first, and its branch is what "merged"
    // means here: no fetch, no remote, nothing that needs the network.
    let base = checkouts.next().and_then(|(_, branch)| branch);
    let mut pruned = Pruned {
        taken: Vec::new(),
        kept: Vec::new(),
        freed_bytes: 0,
        dry_run,
    };
    for (path, branch) in checkouts {
        let facts = facts_of(repo, &path, branch, busy, base.as_deref());
        let worktree = view(&path, facts.branch.clone());
        match keep(&facts) {
            Some(because) => pruned.kept.push(KeptWorktree { worktree, because }),
            None => take(repo, worktree, dry_run, &mut pruned),
        }
    }
    Ok(pruned)
}

/// Measures the worktree, then takes it away unless this is a dry run.
///
/// Measured first, because afterwards there is nothing left to measure. Git
/// refusing to remove it does not fail the prune: that one stays, and is
/// reported as kept, so a locked worktree cannot stop the other nineteen from
/// going.
fn take(repo: &Path, mut worktree: WorktreeView, dry_run: bool, pruned: &mut Pruned) {
    let held = size::measure(Path::new(&worktree.path));
    worktree.size_bytes = Some(i64::try_from(held).unwrap_or(i64::MAX));
    if !dry_run && let Err(err) = take_away(repo, &worktree.path, false) {
        tracing::warn!(path = %worktree.path, %err, "git would not take the worktree away");
        pruned.kept.push(KeptWorktree {
            worktree,
            because: KeptBecause::Refused,
        });
        return;
    }
    pruned.freed_bytes += worktree.size_bytes.unwrap_or(0);
    pruned.taken.push(worktree);
}

/// Forgets Dex's record of the worktrees that went, so the next spawn on one of
/// those branches makes a fresh worktree instead of being sent to a folder that
/// is no longer there.
async fn forget(state: &AppState, repo_id: &str, taken: &[WorktreeView]) -> Result<(), RepoError> {
    for branch in taken.iter().filter_map(|worktree| worktree.branch.clone()) {
        let repo_id = repo_id.to_owned();
        state
            .db
            .call(move |conn| store::detach(conn, &repo_id, &branch))
            .await?;
    }
    Ok(())
}

/// What a worktree's path, branch and tree say about taking it away.
pub struct Facts {
    /// A pane whose shell is running has its folder inside it.
    pub in_use: bool,
    /// Its working tree holds changes git has not been told to keep - or git
    /// would not say, which counts the same.
    pub dirty: bool,
    /// The branch checked out in it, absent when detached.
    pub branch: Option<String>,
    /// Commits its branch has that the main checkout's branch does not, and
    /// `None` when git could not say.
    pub ahead: Option<usize>,
}

/// Why this worktree stays, or `None` when it can go.
///
/// In use first: that a worktree is committed and merged says nothing about
/// whether deleting the folder under a working agent would ruin its afternoon.
pub fn keep(facts: &Facts) -> Option<KeptBecause> {
    if facts.in_use {
        return Some(KeptBecause::InUse);
    }
    if facts.dirty {
        return Some(KeptBecause::Uncommitted);
    }
    if facts.branch.is_none() {
        return Some(KeptBecause::Detached);
    }
    match facts.ahead {
        Some(0) => None,
        Some(_) => Some(KeptBecause::Unmerged),
        None => Some(KeptBecause::Unknown),
    }
}

fn facts_of(
    repo: &Path,
    path: &str,
    branch: Option<String>,
    busy: &[String],
    base: Option<&str>,
) -> Facts {
    let dir = Path::new(path);
    Facts {
        in_use: busy
            .iter()
            .any(|cwd| placement::inside_place(dir, Path::new(cwd))),
        dirty: is_dirty(dir),
        ahead: branch
            .as_deref()
            .zip(base)
            .and_then(|(branch, base)| ahead_of(repo, branch, base)),
        branch,
    }
}

/// Whether the worktree holds anything git has not been told to keep,
/// untracked files included. A tree git would not report on is dirty: it may
/// be mid-rebase, or not there at all, and neither is a folder to delete.
fn is_dirty(dir: &Path) -> bool {
    proc::git(dir, &["status", "--porcelain"]).map_or(true, |out| !out.stdout.is_empty())
}

/// Commits `branch` has that `base` does not. `None` when git could not count
/// them, which keeps the worktree rather than guessing at zero.
fn ahead_of(repo: &Path, branch: &str, base: &str) -> Option<usize> {
    let range = format!("{base}..{branch}");
    let counted = proc::git(repo, &["rev-list", "--count", &range]).ok()?;
    counted.stdout.trim().parse().ok()
}

fn view(path: &str, branch: Option<String>) -> WorktreeView {
    WorktreeView {
        path: paths::normalize(Path::new(path)),
        branch,
        main: false,
        size_bytes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Facts, keep};
    use dex_protocol::repo::KeptBecause;

    /// A worktree with nothing to say for itself: clean, idle, merged.
    fn spent() -> Facts {
        Facts {
            in_use: false,
            dirty: false,
            branch: Some("feat/done".into()),
            ahead: Some(0),
        }
    }

    #[test]
    fn a_clean_merged_worktree_nobody_is_in_can_go() {
        assert_eq!(keep(&spent()), None);
    }

    #[test]
    fn a_worktree_with_a_running_shell_in_it_stays() {
        let busy = Facts {
            in_use: true,
            ..spent()
        };
        assert_eq!(keep(&busy), Some(KeptBecause::InUse));
    }

    #[test]
    fn being_in_use_outweighs_every_other_reason_to_take_it() {
        // The one that matters: an agent working in a worktree whose branch is
        // merged and whose tree is clean is still an agent working in it.
        let working = Facts {
            in_use: true,
            dirty: false,
            branch: Some("feat/done".into()),
            ahead: Some(0),
        };
        assert_eq!(keep(&working), Some(KeptBecause::InUse));
    }

    #[test]
    fn a_worktree_with_uncommitted_work_stays() {
        let dirty = Facts {
            dirty: true,
            ..spent()
        };
        assert_eq!(keep(&dirty), Some(KeptBecause::Uncommitted));
    }

    #[test]
    fn a_worktree_whose_branch_has_its_own_commits_stays() {
        let ahead = Facts {
            ahead: Some(3),
            ..spent()
        };
        assert_eq!(keep(&ahead), Some(KeptBecause::Unmerged));
    }

    #[test]
    fn a_detached_worktree_stays_because_there_is_nothing_to_compare() {
        let detached = Facts {
            branch: None,
            ..spent()
        };
        assert_eq!(keep(&detached), Some(KeptBecause::Detached));
    }

    #[test]
    fn a_branch_git_would_not_compare_keeps_its_worktree() {
        let unknown = Facts {
            ahead: None,
            ..spent()
        };
        assert_eq!(keep(&unknown), Some(KeptBecause::Unknown));
    }
}
