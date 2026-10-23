//! Whether a process started here can follow a directory junction, and what
//! to do when it cannot (issue #102).
//!
//! Windows has a mitigation policy, `ProcessRedirectionTrustPolicy`, that
//! refuses to traverse a mount point a non-admin user created. A process never
//! turns it off: it can only be turned on, and **every child inherits it**.
//! `msiexec.exe` runs with it on, and the MSI's "launch Dex" box - checked by
//! default - starts the app from an installer custom action, so a Dex that
//! begins life at the end of an install has the policy, and so does every pane
//! shell it spawns. Every link pnpm makes in `node_modules` is such a junction,
//! so nothing in a pnpm repo could install, test or build in a Dex pane, with
//! Windows error 448 - `ERROR_UNTRUSTED_MOUNT_POINT` - which Node reports as
//! `UNKNOWN`. An ordinary terminal outside Dex was fine, which is what made it
//! look like Dex had broken its panes.
//!
//! Dex cannot clear the policy, for itself or for a child: clearing it is not
//! something Windows offers, and asking for a child without it means
//! `CreateProcess` attributes that `portable-pty` does not expose and that
//! `#![forbid(unsafe_code)]` rules out anyway. What it can do is not run in
//! such a process: a copy started **through the shell** is Explorer's child,
//! and Explorer has the policy off. That is what the reporter found by hand -
//! starting Dex from the Start menu fixed it - and it is all this module does.
//!
//! The question is asked by trying, not by reading the policy (which would
//! need `unsafe` too): a junction is made under Dex's own data directory and
//! read through. Measured at a median of 12.8 ms for the `mklink` and the read
//! together, 31.8 ms at the worst of ten runs (Ryzen 5 7600, 2026-10-23), once
//! per start.
//!
//! **A limit of asking that way:** the junction is made by whoever Dex runs
//! as, so an elevated Dex makes an admin-owned junction, which the policy
//! trusts and lets through. An elevated Dex with the policy on would read
//! "yes" here and still fail on the links pnpm made unelevated. The reported
//! case is an ordinary user, and that is the one this answers.

/// What to do, having looked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Go on starting in this process.
    CarryOn,
    /// A clean copy is starting; this process must end without binding
    /// anything the new one needs.
    SteppingAside,
}

/// Whether to start a clean copy, given when this process's line last started
/// one and how recently counts as "just now".
///
/// Once, not every time. The policy can also come from machine configuration -
/// a `dex-app.exe` entry in Windows' Exploit Protection, say - and then a copy
/// started through the shell is no cleaner than this one, and each would start
/// another for ever. A restart this recently is the evidence for that, so Dex
/// keeps the process it has and says so in the log.
///
/// A stamp from the future (the clock went back) counts as just now, for the
/// same reason: the cost of waiting is a log line, the cost of looping is the
/// machine.
pub fn start_a_clean_copy(last_started: Option<i64>, now: i64, within: i64) -> bool {
    last_started.is_none_or(|then| now.saturating_sub(then) > within)
}

/// Nothing to do: no Unix kernel refuses a symlink because of who made it.
#[cfg(not(windows))]
pub fn clean_start(_dir: &std::path::Path, _exe: &std::path::Path, _now: i64) -> Start {
    Start::CarryOn
}

#[cfg(windows)]
pub use windows::clean_start;

#[cfg(windows)]
mod windows {
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    use super::{Start, start_a_clean_copy};

    /// How recently a restart has to have been tried for the next one to be
    /// called off. Longer than a hand-off through Explorer takes (a second or
    /// so), short enough that an install tomorrow is answered afresh.
    const WITHIN: Duration = Duration::from_secs(60);

    /// `ERROR_UNTRUSTED_MOUNT_POINT`: "The path cannot be traversed because it
    /// contains an untrusted mount point."
    const UNTRUSTED_MOUNT_POINT: i32 = 448;

    /// Where the last restart is remembered. A file, because the copy is
    /// started by Explorer and not by us: nothing of ours - not an argument,
    /// not an environment variable - reaches it.
    const STAMP: &str = "junction-restart";

    /// What the probe found.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Follow {
        /// A junction this user made can be read through.
        Yes,
        /// Windows refused: the policy is on, here and in every pane shell.
        No,
        /// No junction could be made, so nothing was asked. Not an answer, and
        /// not a reason to restart - the data directory may be on a filesystem
        /// that has no junctions at all.
        CannotTell,
    }

    /// Looks, and if a process here cannot follow a junction, starts a clean
    /// copy of `exe` through the shell. `dir` is Dex's data directory, which
    /// holds both the probe and the stamp.
    pub fn clean_start(dir: &Path, exe: &Path, now: i64) -> Start {
        let stamp = dir.join(STAMP);
        match follows_junctions(dir) {
            Follow::Yes => {
                // So that a later start, in a process that is not this clean,
                // is answered on its own evidence and not on this one's.
                let _ = fs::remove_file(&stamp);
                Start::CarryOn
            }
            Follow::CannotTell => {
                tracing::debug!("no junction could be made to check this process with");
                Start::CarryOn
            }
            Follow::No => step_aside(&stamp, exe, now),
        }
    }

    /// Starts the clean copy, unless one was just started or the stamp cannot
    /// be written - a stamp that never lands would let every copy start
    /// another.
    fn step_aside(stamp: &Path, exe: &Path, now: i64) -> Start {
        let last = fs::read_to_string(stamp)
            .ok()
            .and_then(|text| text.trim().parse::<i64>().ok());
        if !start_a_clean_copy(last, now, WITHIN.as_millis() as i64) {
            tracing::warn!(
                "processes started here cannot follow directory junctions, and a clean copy \
                 has just been started already; carrying on in this one"
            );
            return Start::CarryOn;
        }
        if let Err(err) = fs::write(stamp, now.to_string()) {
            tracing::warn!(%err, "cannot record a restart, so none is attempted");
            return Start::CarryOn;
        }
        match through_the_shell(exe) {
            Ok(()) => {
                tracing::info!(
                    exe = %exe.display(),
                    "started by something that forbids directory junctions; a clean copy is \
                     starting through the shell"
                );
                Start::SteppingAside
            }
            Err(err) => {
                tracing::error!(%err, "no clean copy could be started; panes here cannot follow junctions");
                Start::CarryOn
            }
        }
    }

    /// Hands `exe` to the running shell, so the process it starts is
    /// Explorer's child and inherits Explorer's policy rather than ours.
    /// Measured: a child this process creates itself has the policy, and the
    /// same exe handed over this way starts under the desktop `explorer.exe`
    /// without it (ARCHITECTURE.md).
    ///
    /// **Not waited for.** `explorer.exe` passes the path to the running shell
    /// and exits, but where no shell is running it becomes one and stays, and
    /// a wait would then be a Dex that never appears at all. Nothing needs the
    /// wait: this is before the kill-on-close job (`platform/job.rs`), so the
    /// hand-off is not ours to take down when we end, and Windows does not end
    /// a child because its parent did. Its exit code would say nothing about
    /// what the shell did with the path either way.
    fn through_the_shell(exe: &Path) -> io::Result<()> {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("explorer.exe");
        command.arg(exe);
        // No console window flashing up over the owner's work.
        command.creation_flags(0x0800_0000);
        command.spawn()?;
        Ok(())
    }

    /// Makes a junction under `dir` and reads a file through it.
    ///
    /// One place for it, not one per process: two copies of Dex starting in
    /// the same few milliseconds can take each other's link out from under the
    /// other, and the answer is then "cannot tell", which is a start that goes
    /// on exactly as it does today.
    fn follows_junctions(dir: &Path) -> Follow {
        let probe = dir.join("junction-check");
        let (target, link) = (probe.join("target"), probe.join("link"));
        let readable = target.join("readable");
        if fs::create_dir_all(&target).is_err() || fs::write(&readable, b"junctions\n").is_err() {
            return Follow::CannotTell;
        }
        if make_junction(&link, &target).is_err() {
            return Follow::CannotTell;
        }
        let found = read_through(&link.join("readable"));
        // The link, not the target: `remove_dir` takes a junction out without
        // touching what it points at, and nothing recursive goes near either.
        let _ = fs::remove_dir(&link);
        found
    }

    /// What a read through the junction says about this process.
    fn read_through(path: &Path) -> Follow {
        match fs::read(path) {
            Ok(_) => Follow::Yes,
            Err(err) if err.raw_os_error() == Some(UNTRUSTED_MOUNT_POINT) => Follow::No,
            // Something else went wrong with the probe itself; it says nothing
            // about junctions, so it says nothing at all.
            Err(err) => {
                tracing::debug!(%err, "the junction check could not be read");
                Follow::CannotTell
            }
        }
    }

    /// `mklink /J`, which is a `cmd.exe` builtin: making a junction otherwise
    /// means `DeviceIoControl` and `unsafe`. Any link already there is taken
    /// out first - a junction the policy refuses to traverse looks absent to
    /// anything that asks by following it, so it is removed rather than tested
    /// for.
    fn make_junction(link: &Path, target: &Path) -> io::Result<()> {
        use std::os::windows::process::CommandExt;
        let _ = fs::remove_dir(link);
        let mut command = Command::new("cmd.exe");
        command.args(["/c", "mklink", "/J"]).arg(link).arg(target);
        command.creation_flags(0x0800_0000);
        let made = command.output()?;
        if made.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(
                String::from_utf8_lossy(&made.stderr).trim().to_owned(),
            ))
        }
    }

    #[cfg(test)]
    mod tests {
        use std::fs;

        use super::{Follow, Start, UNTRUSTED_MOUNT_POINT, clean_start, follows_junctions};

        /// The answer on an ordinary machine, which is also the answer that
        /// must not be a false alarm: a junction made here reads back, so
        /// nothing restarts. The other answer cannot be staged from a test -
        /// no process can turn the policy on for itself without `unsafe` - and
        /// was proved by hand instead, on the issue.
        #[test]
        fn a_junction_this_user_made_can_be_read_through() {
            let dir = tempfile::tempdir().expect("a temp dir");
            assert_eq!(follows_junctions(dir.path()), Follow::Yes);
        }

        /// And the probe leaves no junction behind in the data directory.
        #[test]
        fn the_check_takes_its_junction_out_again() {
            let dir = tempfile::tempdir().expect("a temp dir");
            follows_junctions(dir.path());
            let link = dir.path().join("junction-check").join("link");
            assert!(
                fs::symlink_metadata(&link).is_err(),
                "the junction is still there: {}",
                link.display()
            );
        }

        /// A place no junction can be made in is not an answer either way. A
        /// file stands in for every such place - a data directory on a
        /// filesystem without junctions, or one Dex may not write to - because
        /// it is the one a test can be sure of on any machine.
        #[test]
        fn nowhere_to_make_one_tells_us_nothing() {
            let dir = tempfile::tempdir().expect("a temp dir");
            let not_a_directory = dir.path().join("in-the-way");
            fs::write(&not_a_directory, b"a file, not a directory\n").expect("a file in the way");
            assert_eq!(follows_junctions(&not_a_directory), Follow::CannotTell);
        }

        /// The whole path, on a machine whose processes can follow junctions:
        /// Dex starts in the process it has, and nothing is launched. The exe
        /// named does not exist, so a machine that did restart would fail
        /// visibly here rather than starting something.
        #[test]
        fn a_process_that_can_follow_junctions_carries_on() {
            let dir = tempfile::tempdir().expect("a temp dir");
            let stamp = dir.path().join("junction-restart");
            fs::write(&stamp, "1").expect("a stamp to forget");
            let exe = dir.path().join("no-such-dex-app.exe");
            assert_eq!(clean_start(dir.path(), &exe, 2), Start::CarryOn);
            assert!(
                !stamp.exists(),
                "a restart from some earlier start is still remembered"
            );
        }

        /// The error code the policy answers with, as Windows spells it: a
        /// number that named some other error would leave the check above
        /// answering "cannot tell" for ever and nothing would ever restart.
        ///
        /// It reads Windows' own words for the code, so it wants an English
        /// system; it is pinned this way because a process cannot turn the
        /// policy on for itself without `unsafe`, and there is nothing else
        /// here to compare against.
        #[test]
        fn the_code_is_the_one_windows_documents() {
            let err = std::io::Error::from_raw_os_error(UNTRUSTED_MOUNT_POINT);
            assert!(
                err.to_string().to_lowercase().contains("mount point"),
                "{UNTRUSTED_MOUNT_POINT} is not the untrusted mount point error here, \
                 or this system is not in English: {err}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::start_a_clean_copy;

    /// A minute, in the unit the rule takes.
    const WITHIN: i64 = 60_000;

    #[test]
    fn a_process_that_never_started_a_copy_starts_one() {
        assert!(start_a_clean_copy(None, 1_000, WITHIN));
    }

    #[test]
    fn a_copy_just_started_is_not_started_again() {
        assert!(!start_a_clean_copy(Some(1_000), 1_500, WITHIN));
    }

    /// The boundary belongs to the copy that was started: at exactly a minute
    /// the hand-off is long done, so this is the first moment that says
    /// nothing, and it says nothing.
    #[test]
    fn a_copy_started_exactly_a_minute_ago_is_not_started_again() {
        assert!(!start_a_clean_copy(Some(1_000), 1_000 + WITHIN, WITHIN));
    }

    #[test]
    fn a_copy_started_longer_ago_than_that_says_nothing_about_now() {
        assert!(start_a_clean_copy(Some(1_000), 1_001 + WITHIN, WITHIN));
    }

    /// A clock that went back leaves a stamp in the future. Read as "just
    /// now", which calls the restart off; the alternative is a machine
    /// starting copies of Dex until someone stops it.
    #[test]
    fn a_stamp_from_the_future_counts_as_just_now() {
        assert!(!start_a_clean_copy(Some(9_000), 1_000, WITHIN));
    }
}
