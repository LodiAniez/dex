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
//!
//! The facts are gathered here; the rules that read them are pure, in
//! `verdict.rs`, and what the remote says became of a branch is `landed.rs`.

use std::path::Path;

use dex_protocol::repo::{KeptBecause, KeptWorktree, PruneWorktreesArgs, Pruned, WorktreeView};

use super::commands::resolve;
use super::landed;
use super::model::RepoError;
use super::verdict::{self, Facts};
use super::worktree_commands::take_away;
use super::{logic, placement, size, store};
use crate::app::AppState;
use crate::features::workspace;
use crate::platform::clock;
use crate::platform::paths;
use crate::platform::proc::{self, GitError};

/// `worktree.prune`: removes every worktree of a repo that is safe to remove,
/// and says what was left behind and why.
pub async fn prune(state: &AppState, args: PruneWorktreesArgs) -> Result<Pruned, RepoError> {
    let repo = resolve(state, &args.repo).await?;
    let busy = busy_folders(state).await?;
    // In auto mode the remote decides what has landed, and anything worked in
    // inside the grace period is left alone whatever it says.
    let grace = args
        .auto
        .then(|| (state.config.get().agents.prune_after_hours as i64).saturating_mul(3_600));
    let (repo_path, dry_run) = (repo.path.clone(), args.dry_run);
    let pruned =
        tokio::task::spawn_blocking(move || sweep(Path::new(&repo_path), &busy, dry_run, grace))
            .await
            .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))??;
    if !pruned.dry_run {
        forget(state, &repo.id, &pruned.taken).await;
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

/// What one sweep of a repository needs in order to judge every worktree in it.
struct Judging<'a> {
    repo: &'a Path,
    /// The folders panes with a running shell are in.
    busy: &'a [String],
    /// `Some(seconds)` in auto mode: how long after work stops a worktree is
    /// still left alone. `None` outside it, where the owner asked directly.
    grace: Option<i64>,
    /// The remote and its branches, asked once for the whole repository.
    /// `None` outside auto mode, when there was nothing to judge, or when the
    /// remote could not be reached.
    on_remote: Option<landed::OnRemote>,
    /// Seconds since the epoch, read once, so every worktree in one sweep is
    /// judged against the same moment.
    now: i64,
    /// The main checkout's branch, for the local comparison.
    base: Option<String>,
}

/// Judges every worktree of the repository, main checkout aside, and takes
/// away the ones nothing speaks for.
fn sweep(
    repo: &Path,
    busy: &[String],
    dry_run: bool,
    grace: Option<i64>,
) -> Result<Pruned, RepoError> {
    let listed = proc::git(repo, &["worktree", "list", "--porcelain"])?;
    let mut checkouts = logic::parse_worktrees(&listed.stdout).into_iter();
    // Git lists the main checkout first, and its branch is what the local
    // comparison means by merged.
    let base = checkouts.next().and_then(|(_, branch)| branch);
    let checkouts: Vec<(String, Option<String>)> = checkouts.collect();
    let judging = Judging {
        repo,
        busy,
        grace,
        // The only part of the prune that touches the network: only in auto
        // mode, and only when there is a worktree to judge, so a repository
        // with nothing but its main checkout costs no call at all (review). A
        // remote that cannot be reached leaves every worktree to the local
        // comparison.
        on_remote: grace
            .filter(|_| !checkouts.is_empty())
            .and_then(|_| landed::branches_on_remote(repo)),
        now: clock::now_millis() / 1_000,
        base,
    };
    let mut pruned = Pruned {
        taken: Vec::new(),
        kept: Vec::new(),
        taken_bytes: 0,
        dry_run,
    };
    for (path, branch) in checkouts {
        let facts = facts_of(&judging, &path, branch);
        let worktree = view(&path, facts.branch.clone());
        match verdict::keep(&facts) {
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
    pruned.taken_bytes += worktree.size_bytes.unwrap_or(0);
    pruned.taken.push(worktree);
}

/// Forgets Dex's record of the worktrees that went, so the next spawn on one of
/// those branches makes a fresh worktree instead of being sent to a folder that
/// is no longer there.
///
/// Best effort: the folders are already gone, and a record left behind is a
/// spawn that has to make its worktree again, which is worth a warning rather
/// than an error that hides what the prune just did.
async fn forget(state: &AppState, repo_id: &str, taken: &[WorktreeView]) {
    for branch in taken.iter().filter_map(|worktree| worktree.branch.clone()) {
        let repo_id = repo_id.to_owned();
        let forgotten = state
            .db
            .call(move |conn| store::detach(conn, &repo_id, &branch))
            .await;
        if let Err(err) = forgotten {
            tracing::warn!(%err, "a worktree went but Dex could not forget its record");
        }
    }
}

fn facts_of(judging: &Judging<'_>, path: &str, branch: Option<String>) -> Facts {
    let dir = Path::new(path);
    let present = dir.is_dir();
    // Before `is_dirty`, which runs git inside the worktree: nothing read here
    // may be disturbed by Dex's own reading of it.
    let worked_recently = present && recently_used(judging, dir, branch.as_deref());
    Facts {
        in_use: judging
            .busy
            .iter()
            .any(|cwd| placement::inside_place(dir, Path::new(cwd))),
        present,
        dirty: present && is_dirty(dir),
        recently_used: worked_recently,
        landed: branch.as_deref().and_then(|branch| {
            judging
                .grace
                .map(|_| landed::landed(judging.repo, branch, judging.on_remote.as_ref()))
        }),
        ahead: branch
            .as_deref()
            .zip(judging.base.as_deref())
            .and_then(|(branch, base)| ahead_of(judging.repo, branch, base)),
        branch,
    }
}

/// Whether something was working in the worktree inside the grace period.
///
/// Outside auto mode nobody asked, and the answer is no. Inside it, a worktree
/// whose timestamps cannot be read counts as worked in: this slice keeps what
/// it cannot judge.
fn recently_used(judging: &Judging<'_>, dir: &Path, branch: Option<&str>) -> bool {
    let Some(grace) = judging.grace else {
        return false;
    };
    match last_worked(judging.repo, dir, branch) {
        Some(at) => judging.now - at < grace,
        None => true,
    }
}

/// When work last happened in the worktree, in seconds since the epoch: the
/// newest of the folder's own timestamp, those of everything directly in it,
/// and the branch's last commit.
///
/// None of those moves when Dex reads the worktree, which is the whole
/// requirement. `git status` can rewrite the index git keeps for a worktree, so
/// an index timestamp would be refreshed by this prune's own dirty check and
/// every worktree would look busy for ever.
///
/// The entries just inside as well as the folder, because the two say
/// different things (both measured, review): a folder's timestamp moves when
/// something is **added to or removed from it**, so the folder alone misses a
/// top-level file being edited - a regenerated `pnpm-lock.yaml`, a
/// hand-written `.env` - while that file's own timestamp catches it. One
/// `read_dir`, no recursion: the top of a checkout is a handful of entries.
///
/// A commit moves the branch; and a worktree a spawn made moments ago is new by
/// its folder, which is what keeps it from being taken in the seconds before
/// its shell starts.
///
/// **What none of this sees** is a process Dex does not own writing further
/// down, under a folder git ignores: a dev server rewriting
/// `node_modules/.vite` moves `.vite` and nothing above it, and no timestamp
/// short of walking the whole tree would notice - and walking it is what this
/// exists to avoid. What protects that worktree instead is everything else the
/// verdict asks: its tree is clean, so nothing git tracks is lost; its branch
/// is gone from the remote, so its history is safe there; and the grace period
/// runs from whichever is newer of its creation and its last commit. What can
/// still be lost is an ignored file nobody committed, which is equally true of
/// `git worktree remove` and is why the prune's help says so.
fn last_worked(repo: &Path, dir: &Path, branch: Option<&str>) -> Option<i64> {
    let newest = touched(dir).max(
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| touched(&entry.path()))
            .max(),
    );
    let commit = branch.and_then(|branch| {
        let shown = proc::git(repo, &["log", "-1", "--format=%ct", branch]).ok()?;
        shown.stdout.trim().parse::<i64>().ok()
    });
    newest.max(commit)
}

/// When a file or folder was last written, in seconds since the epoch.
fn touched(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs()).ok())
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
    use std::path::Path;

    use super::last_worked;

    #[test]
    fn editing_a_file_at_the_top_of_a_worktree_counts_as_work_in_it() {
        // A folder's own timestamp does not move when a file already in it is
        // written - only when an entry is added or removed - so the folder
        // alone would call this worktree cold (review).
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "KEY=one\n").unwrap();
        let before = last_worked(Path::new("."), dir.path(), None).expect("the folder is there");

        std::thread::sleep(std::time::Duration::from_millis(1_100));
        std::fs::write(dir.path().join(".env"), "KEY=two\n").unwrap();
        let after = last_worked(Path::new("."), dir.path(), None).expect("still there");

        assert!(after > before, "the edit at {after} is newer than {before}");
    }

    #[test]
    fn a_folder_that_is_not_there_was_never_worked_in() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            last_worked(Path::new("."), &dir.path().join("gone"), None),
            None
        );
    }
}
