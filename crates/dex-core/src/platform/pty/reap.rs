//! Ending everything a pane started, on macOS and Linux.
//!
//! Windows needs none of this: a pane's processes are in Dex's kill-on-close
//! job. On Unix, closing a terminal hangs up only its foreground job, and an
//! interactive shell puts each job - `claude`, a dev server started in the
//! background - in a process group of its own. So Dex hangs up every process
//! under the pane's shell, waits a moment for them to go, and kills the rest.
//! A process that detached itself (double fork, `setsid`) is no longer under
//! the shell and is not found; nothing short of a job object would find it.

use crate::platform::proctree::Proc;

/// How long a closed pane's processes get to go after the hang-up.
#[cfg(unix)]
pub(super) const CLOSE_GRACE: std::time::Duration = std::time::Duration::from_secs(3);
/// The same as the app quits, which waits for it.
#[cfg(unix)]
const QUIT_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

/// The pane's shell and every process under it, shell first.
pub(super) fn tree(procs: &[Proc], root: u32) -> Vec<Proc> {
    let mut found: Vec<Proc> = procs.iter().filter(|p| p.pid == root).cloned().collect();
    let mut next = 0;
    while next < found.len() {
        let parent = found[next].pid;
        found.extend(
            procs
                .iter()
                .filter(|p| {
                    p.parent == parent && p.pid != parent && !found.iter().any(|f| f.pid == p.pid)
                })
                .cloned()
                .collect::<Vec<_>>(),
        );
        next += 1;
    }
    found
}

/// Which of `tree` are still running in `now`: the same pid with the same
/// command line, so a pid the system has since reused is left alone.
pub(super) fn survivors(tree: &[Proc], now: &[Proc]) -> Vec<u32> {
    tree.iter()
        .filter(|old| {
            now.iter()
                .any(|p| p.pid == old.pid && p.command == old.command)
        })
        .map(|old| old.pid)
        .collect()
}

#[cfg(unix)]
pub(super) use unix::{end_all, end_tree};

#[cfg(unix)]
mod unix {
    use std::thread;
    use std::time::{Duration, Instant};

    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    use super::{QUIT_GRACE, survivors, tree};
    use crate::platform::proctree;

    fn send(pid: u32, signal: Signal) {
        if let Ok(pid) = i32::try_from(pid) {
            // Gone already is the outcome we want.
            let _ = kill(Pid::from_raw(pid), signal);
        }
    }

    /// Ends every pane at once as the app quits, waiting for them all.
    pub fn end_all(shells: Vec<u32>) {
        let ending: Vec<_> = shells
            .into_iter()
            .map(|pid| thread::spawn(move || end_tree(pid, QUIT_GRACE)))
            .collect();
        for thread in ending {
            let _ = thread.join();
        }
    }

    /// Hangs up the shell `root` and everything under it, then after `grace`
    /// kills whatever is still running. Blocks for up to `grace`.
    pub fn end_tree(root: u32, grace: Duration) {
        let Ok(procs) = proctree::snapshot() else {
            send(root, Signal::SIGHUP);
            return;
        };
        let tree = tree(&procs, root);
        for proc in &tree {
            send(proc.pid, Signal::SIGHUP);
        }
        let deadline = Instant::now() + grace;
        loop {
            thread::sleep(Duration::from_millis(100));
            let Ok(now) = proctree::snapshot() else {
                return;
            };
            let left = survivors(&tree, &now);
            if left.is_empty() {
                return;
            }
            if Instant::now() >= deadline {
                tracing::debug!(?left, "pane processes ignored the hang-up; killing them");
                for pid in left {
                    send(pid, Signal::SIGKILL);
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: u32, command: &str) -> Proc {
        Proc {
            pid,
            parent,
            name: command.split(' ').next().unwrap_or_default().to_owned(),
            command: command.to_owned(),
        }
    }

    #[test]
    fn a_pane_s_tree_is_its_shell_and_everything_under_it_and_nothing_else() {
        let procs = [
            proc(1, 0, "launchd"),
            proc(10, 1, "dex-app"),
            proc(20, 10, "-zsh"),
            proc(30, 20, "node claude"),
            proc(40, 30, "npm run dev"),
            proc(41, 40, "vite"),
            proc(50, 10, "-zsh"),
            proc(60, 50, "vim"),
        ];
        let pids: Vec<u32> = tree(&procs, 20).iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![20, 30, 40, 41]);
        assert!(tree(&procs, 999).is_empty());
    }

    #[test]
    fn only_processes_still_running_as_themselves_are_killed() {
        let tree = [
            proc(20, 10, "-zsh"),
            proc(30, 20, "node claude"),
            proc(40, 30, "vite"),
        ];
        // 20 went; 30 still runs (now under launchd); 40's pid went to someone else.
        let now = [proc(30, 1, "node claude"), proc(40, 1, "Safari")];
        assert_eq!(survivors(&tree, &now), vec![30]);
    }
}
