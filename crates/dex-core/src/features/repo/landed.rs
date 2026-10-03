//! Whether a branch's work has landed, asked of the remote rather than worked
//! out from the branch graph (issue #74).
//!
//! The local test - is this branch an ancestor of the main one - is the obvious
//! one and it is wrong for the way most work is merged. A squash merge writes a
//! **new** commit, so afterwards the branch is not an ancestor of anything:
//! `git merge-base --is-ancestor` says no and `rev-list --count main..branch`
//! stays above zero for ever. A prune built on it would never take a single
//! worktree whose pull request had been squashed, and loosening it to make it
//! fire would take unmerged work. `git cherry` is no better: it matches a patch
//! id per commit, and a squash of five commits has an id matching none of them.
//!
//! So this asks the remote. A branch that was pushed and is now gone from the
//! remote has landed, however it got there - merged, squashed, rebased - or has
//! been abandoned on purpose, and either way the agent is finished with it.
//! A branch with no remote-tracking ref was never pushed, and may hold the only
//! copy of its work: that one is never taken.

use std::path::Path;

use crate::platform::proc;

/// What the remote says about a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    /// Pushed once, gone from the remote now, and nothing was committed since:
    /// everything in the worktree is in the history the remote accepted.
    Yes,
    /// The remote still has the branch, so the work has not landed.
    StillThere,
    /// No remote-tracking ref, or commits beyond it: the only copy of some of
    /// this work is here.
    NotPushed,
    /// There is no remote, or it could not be reached, or git would not say.
    CannotTell,
}

/// What a repository's remote has: its name, and every branch on it. `None`
/// when there is no remote to ask or it could not be reached.
///
/// One call per repository rather than per branch, and through
/// `git_unattended`, because this is the only thing in the prune that touches
/// the network and nobody is there to type a password. The name comes back with
/// the listing so that judging a branch costs no further subprocesses: asking
/// git for it again per branch was two extra processes each (review).
pub fn branches_on_remote(repo: &Path) -> Option<OnRemote> {
    let remote = first_remote(repo)?;
    match proc::git_unattended(repo, &["ls-remote", "--heads", &remote]) {
        Ok(out) => Some(OnRemote {
            branches: parse_heads(&out.stdout),
            remote,
        }),
        Err(err) => {
            tracing::debug!(%remote, %err, "could not ask the remote which branches it has");
            None
        }
    }
}

/// A remote, and the branches it had when it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnRemote {
    /// The remote's name, for naming its tracking refs.
    pub remote: String,
    /// Every branch it listed.
    pub branches: Vec<String>,
}

/// What the remote says about one branch, given the listing for the repository.
pub fn landed(repo: &Path, branch: &str, on_remote: Option<&OnRemote>) -> Landed {
    let Some(on_remote) = on_remote else {
        return Landed::CannotTell;
    };
    if on_remote.branches.iter().any(|head| head == branch) {
        return Landed::StillThere;
    }
    // Gone from the remote - but a branch that was never pushed is also not
    // there, and that is the one case where deleting the folder loses work.
    let tracking = format!("refs/remotes/{}/{branch}", on_remote.remote);
    if !has_ref(repo, &tracking) {
        return Landed::NotPushed;
    }
    match unpushed(repo, &tracking, branch) {
        Some(0) => Landed::Yes,
        Some(_) => Landed::NotPushed,
        None => Landed::CannotTell,
    }
}

/// The branch names in `git ls-remote --heads` output.
///
/// Each line is `<sha>\trefs/heads/<name>`. A name with slashes in it keeps
/// them, and anything that is not a head is ignored.
pub fn parse_heads(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter_map(|(_, reference)| reference.trim().strip_prefix("refs/heads/"))
        .map(str::to_owned)
        .collect()
}

/// The remote to ask: the branch's own if it has one, else `origin` if there is
/// one, else the first git lists. `None` when the repository has no remote.
fn first_remote(repo: &Path) -> Option<String> {
    let listed = proc::git(repo, &["remote"]).ok()?;
    let remotes: Vec<&str> = listed
        .stdout
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect();
    if remotes.contains(&"origin") {
        return Some("origin".to_owned());
    }
    remotes.first().map(|remote| (*remote).to_owned())
}

fn has_ref(repo: &Path, reference: &str) -> bool {
    proc::git(repo, &["rev-parse", "--verify", "--quiet", reference]).is_ok()
}

/// Commits on `branch` that the remote never took. `None` when git would not
/// count them, which this slice always reads as "keep the worktree".
fn unpushed(repo: &Path, tracking: &str, branch: &str) -> Option<usize> {
    let range = format!("{tracking}..{branch}");
    let counted = proc::git(repo, &["rev-list", "--count", &range]).ok()?;
    counted.stdout.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{Landed, OnRemote, landed, parse_heads};
    use std::path::Path;

    #[test]
    fn the_heads_a_remote_lists_are_read_by_name() {
        let listing = "\
9f8e7d6c5b4a39281706f5e4d3c2b1a098765432\trefs/heads/main
1122334455667788990011223344556677889900\trefs/heads/feat/pick-table
aabbccddeeff00112233445566778899aabbccdd\trefs/heads/fix/login
deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\trefs/tags/v1.0
";
        assert_eq!(
            parse_heads(listing),
            vec!["main", "feat/pick-table", "fix/login"],
            "tags and anything else are not branches"
        );
    }

    #[test]
    fn nothing_from_an_empty_or_odd_listing() {
        assert!(parse_heads("").is_empty());
        assert!(parse_heads("not a listing at all\n").is_empty());
    }

    #[test]
    fn a_remote_that_could_not_be_asked_tells_us_nothing() {
        // The whole point of the variant: no answer must never read as "gone".
        assert_eq!(
            landed(Path::new("."), "feat/x", None),
            Landed::CannotTell,
            "no listing means no judgement"
        );
    }

    #[test]
    fn a_branch_the_remote_still_has_has_not_landed() {
        let listed = OnRemote {
            remote: "origin".into(),
            branches: vec!["main".to_owned(), "feat/x".to_owned()],
        };
        assert_eq!(
            landed(Path::new("."), "feat/x", Some(&listed)),
            Landed::StillThere
        );
    }
}
