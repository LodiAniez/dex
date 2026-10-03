//! Whether a worktree can be taken away, and why not (issues #70, #74).
//!
//! The rules alone, with no git and no file system in them: `prune.rs` gathers
//! the facts and this decides. Every question that cannot be answered is
//! answered "keep it", because a worktree may hold the only copy of an
//! afternoon's work and the other mistake only costs disk.

use dex_protocol::repo::KeptBecause;

use super::landed::Landed;

/// What a worktree's path, branch and tree say about taking it away.
pub struct Facts {
    /// A pane whose shell is running has its folder inside it.
    pub in_use: bool,
    /// Its folder is there to look at. A worktree on a distro that is not
    /// running is not, and nothing can be judged about it from here.
    pub present: bool,
    /// Its working tree holds changes git has not been told to keep - or git
    /// would not say, which counts the same.
    pub dirty: bool,
    /// The branch checked out in it, absent when detached.
    pub branch: Option<String>,
    /// Commits its branch has that the main checkout's branch does not, and
    /// `None` when git could not say.
    pub ahead: Option<usize>,
    /// What the remote says became of its branch. `None` outside auto mode,
    /// where nobody asked and the local comparison decides.
    pub landed: Option<Landed>,
    /// Something was working in it within the grace period. Always false
    /// outside auto mode.
    pub recently_used: bool,
}

/// Why this worktree stays, or `None` when it can go.
///
/// In use first: that a worktree is committed and merged says nothing about
/// whether deleting the folder under a working agent would ruin its afternoon.
pub fn keep(facts: &Facts) -> Option<KeptBecause> {
    if facts.in_use {
        return Some(KeptBecause::InUse);
    }
    // Before anything git is asked, because git cannot be asked: with the
    // folder away, `status` fails and that would read as uncommitted work.
    if !facts.present {
        return Some(KeptBecause::Missing);
    }
    if facts.dirty {
        return Some(KeptBecause::Uncommitted);
    }
    // Before the remote is believed: a branch merged an hour ago is a branch
    // whose diff the owner may still want to read, and whose agent may be
    // part-way through its next turn. This is also what keeps a worktree a
    // spawn has just made from being swept away before its shell starts.
    if facts.recently_used {
        return Some(KeptBecause::RecentlyUsed);
    }
    if facts.branch.is_none() {
        return Some(KeptBecause::Detached);
    }
    match facts.landed {
        Some(Landed::Yes) => None,
        Some(Landed::StillThere) => Some(KeptBecause::StillOpen),
        Some(Landed::NotPushed) => Some(KeptBecause::Unpushed),
        // No remote to ask, or none that answered: back to the local
        // comparison, which is right about a branch with no commits of its own
        // whatever the merge style was, and keeps everything else.
        Some(Landed::CannotTell) | None => match facts.ahead {
            Some(0) => None,
            Some(_) => Some(KeptBecause::Unmerged),
            None => Some(KeptBecause::Unknown),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{Facts, Landed, keep};
    use dex_protocol::repo::KeptBecause;

    /// A worktree with nothing to say for itself: clean, idle, merged, and
    /// nobody asked the remote about it.
    fn spent() -> Facts {
        Facts {
            in_use: false,
            present: true,
            dirty: false,
            branch: Some("feat/done".into()),
            ahead: Some(0),
            landed: None,
            recently_used: false,
        }
    }

    /// The same worktree as the hourly pass sees it: the remote was asked, and
    /// the branch has been merged and deleted there. Cold for a day.
    fn landed_branch() -> Facts {
        Facts {
            landed: Some(Landed::Yes),
            // Squash-merged, so locally it still looks ahead of main - which is
            // the whole reason the remote is asked at all.
            ahead: Some(4),
            ..spent()
        }
    }

    #[test]
    fn a_branch_the_remote_has_taken_can_go_however_it_looks_locally() {
        // A squash merge leaves the branch ahead of main for ever. Judged on
        // `ahead`, this feature would never fire once.
        assert_eq!(keep(&landed_branch()), None);
    }

    #[test]
    fn a_branch_the_remote_still_has_stays() {
        let open = Facts {
            landed: Some(Landed::StillThere),
            ..landed_branch()
        };
        assert_eq!(keep(&open), Some(KeptBecause::StillOpen));
    }

    #[test]
    fn a_branch_no_remote_ever_took_stays() {
        // The dangerous one: absent from the remote because it was never
        // pushed there, so the folder holds the only copy.
        let unpushed = Facts {
            landed: Some(Landed::NotPushed),
            ..landed_branch()
        };
        assert_eq!(keep(&unpushed), Some(KeptBecause::Unpushed));
    }

    #[test]
    fn work_in_the_last_few_hours_outlives_even_a_merged_branch() {
        let fresh = Facts {
            recently_used: true,
            ..landed_branch()
        };
        assert_eq!(keep(&fresh), Some(KeptBecause::RecentlyUsed));
    }

    #[test]
    fn a_remote_that_would_not_answer_falls_back_to_the_local_comparison() {
        let silent = Facts {
            landed: Some(Landed::CannotTell),
            ..landed_branch()
        };
        assert_eq!(
            keep(&silent),
            Some(KeptBecause::Unmerged),
            "four commits the main branch has not: keep it"
        );
        let nothing_of_its_own = Facts {
            landed: Some(Landed::CannotTell),
            ahead: Some(0),
            ..landed_branch()
        };
        assert_eq!(keep(&nothing_of_its_own), None);
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
            ..landed_branch()
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
    fn a_worktree_whose_folder_has_gone_is_kept_and_named_as_that() {
        // Not as uncommitted work: with the folder away `git status` fails,
        // and saying "uncommitted" of a folder that is not there sends the
        // owner looking for work that is not in it. On Windows this is every
        // worktree on a distro that has been shut down.
        let gone = Facts {
            present: false,
            ..spent()
        };
        assert_eq!(keep(&gone), Some(KeptBecause::Missing));
    }

    #[test]
    fn a_shell_running_in_it_still_wins_over_a_folder_that_seems_gone() {
        let odd = Facts {
            in_use: true,
            present: false,
            ..spent()
        };
        assert_eq!(keep(&odd), Some(KeptBecause::InUse));
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
