//! Typing into a pane, without holding every other pane while it happens.
//!
//! A PTY write takes real time, and can take for ever. On a Unix pty it
//! blocks as soon as the child stops reading its input and the buffer fills;
//! on Windows ConPTY keeps absorbing input from a live child, about a
//! megabyte a second, and blocks only once the pseudoconsole is gone. Either
//! way the map of live panes is shared by the whole daemon, and a write that
//! held it would stop every other pane being typed into, resized, opened,
//! killed, or asked when it last spoke - which the watchdog asks of every
//! running agent, every sweep. One stuck pane looked like a frozen app
//! (issue #63).
//!
//! So each pane's writer has a lock of its own, and the map is held only long
//! enough to take a handle to it. That lock is mutual exclusion, not a queue:
//! it keeps two writes to one pane from interleaving, while what puts text
//! before the Enter after it is `pane_io::send`, which awaits the first write
//! before making the second.

use std::io::Write;
use std::sync::{Arc, Mutex};

use super::{Panes, PtyError};

/// A pane's end of the PTY, behind its own lock.
pub(super) type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// Takes a freshly opened PTY's writer for its pane to hold.
pub(super) fn hold(writer: Box<dyn Write + Send>) -> Writer {
    Arc::new(Mutex::new(writer))
}

/// A handle to the pane's writer, with the map released before the caller
/// writes anything through it.
fn writer_of(panes: &Panes, pane_id: &str) -> Option<Writer> {
    // A poisoned lock means a thread panicked mid-update; the map itself is
    // still usable, and refusing all PTY I/O would be worse.
    let panes = panes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    panes.get(pane_id).map(|pane| pane.writer.clone())
}

/// Sends input to a pane, given the map directly. `watch` answers prompts from
/// its own thread and has no supervisor to call.
/// The map must not be held here: the write is what this module exists to
/// keep out of it, and `failure` looks in the map again afterwards.
pub(super) fn write_to(panes: &Panes, pane_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
    let writer =
        writer_of(panes, pane_id).ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
    let failed = {
        let mut writer = writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        writer.write_all(bytes).and_then(|()| writer.flush()).err()
    };
    match failed {
        None => Ok(()),
        Some(err) => Err(failure(panes, pane_id, err)),
    }
}

/// What a failed write means. The handle outlives its place in the map, so a
/// pane that exited while this was being written fails as a dying PTY - which
/// is the pane going, not a fault to report as one. The map, read now, says
/// which of the two it was.
fn failure(panes: &Panes, pane_id: &str, err: std::io::Error) -> PtyError {
    match writer_of(panes, pane_id) {
        Some(_) => err.into(),
        None => PtyError::NoSuchPane(pane_id.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc::channel;
    use std::thread;
    use std::time::Duration;

    use super::super::tests::{cmd, collecting_sink};
    use super::super::{FlowLimits, PtyError, PtySupervisor, SpawnRequest};
    use super::writer_of;

    /// A child that sits and reads its input: cmd.exe on Windows, /bin/sh
    /// elsewhere. Both stay alive until their pane is killed.
    fn interactive(pane_id: &str) -> SpawnRequest {
        SpawnRequest {
            program: if cfg!(windows) {
                PathBuf::from("cmd.exe")
            } else {
                PathBuf::from("/bin/sh")
            },
            args: Vec::new(),
            ..cmd(pane_id, "")
        }
    }

    /// Issue #63: while one pane is being written to - slowly on Windows,
    /// blocked outright on a Unix pty whose child has stopped reading - every
    /// other pane must stay usable.
    #[test]
    fn a_long_write_to_one_pane_does_not_hold_up_another() {
        let supervisor = PtySupervisor::new(FlowLimits::default());
        let (sink_slow, _slow_out) = collecting_sink();
        let (sink_busy, _busy_out) = collecting_sink();
        supervisor.spawn(interactive("slow"), sink_slow).unwrap();
        supervisor.spawn(interactive("busy"), sink_busy).unwrap();

        let writing = supervisor.clone();
        let (finished, has_finished) = channel();
        thread::spawn(move || {
            // Megabytes, so this is still going while we type into the other
            // pane: seconds at ConPTY's measured megabyte a second, and on a
            // Unix pty it blocks within the first few kilobytes.
            let _ = writing.write("slow", &vec![b'x'; 2 * 1024 * 1024]);
            let _ = finished.send(());
        });

        // Until that write holds the pane's writer, not merely until its
        // thread has been scheduled: a machine that has not got to it yet
        // would make everything below pass for the wrong reason.
        let slow = writer_of(&supervisor.panes, "slow").expect("the slow pane");
        let mut under_way = false;
        for _ in 0..500 {
            if slow.try_lock().is_err() {
                under_way = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(under_way, "the long write never started");

        let typing = supervisor.clone();
        let (done, landed) = channel();
        thread::spawn(move || {
            let _ = done.send(typing.write("busy", b"\r").is_ok());
        });
        assert_eq!(
            landed.recv_timeout(Duration::from_millis(1500)),
            Ok(true),
            "typing into another pane waited on the long write"
        );
        assert!(
            has_finished.try_recv().is_err(),
            "the long write was over already; this proves nothing"
        );

        // The panes go; that write does not. Measured on Windows: a write
        // already inside ConPTY's input pipe never returns once the
        // pseudoconsole is closed, so its thread outlives this test.
        supervisor.kill("slow").ok();
        supervisor.kill("busy").ok();
    }

    /// The other half: the writer's lock is mutual exclusion, so two writes to
    /// one pane cannot interleave. Which goes first is not this lock's to say
    /// - `pane_io::send` awaits its text before it sends the Enter.
    #[test]
    fn a_second_write_to_one_pane_waits_for_the_first() {
        let supervisor = PtySupervisor::new(FlowLimits::default());
        let (sink, _out) = collecting_sink();
        supervisor
            .spawn(interactive("one-at-a-time"), sink)
            .unwrap();
        let writer = writer_of(&supervisor.panes, "one-at-a-time").expect("the pane");
        let in_flight = writer.lock().unwrap();

        let typing = supervisor.clone();
        let (done, landed) = channel();
        thread::spawn(move || {
            let _ = done.send(typing.write("one-at-a-time", b"\r").is_ok());
        });
        assert!(
            landed.recv_timeout(Duration::from_millis(300)).is_err(),
            "a second write to the same pane went in while the first was in flight"
        );

        drop(in_flight);
        // And it really was that thread waiting, not one that never ran.
        assert_eq!(
            landed.recv_timeout(Duration::from_secs(5)),
            Ok(true),
            "it goes in once the first is done"
        );
        supervisor.kill("one-at-a-time").ok();
    }

    /// A writer that stops mid-write until its pane has been taken out of the
    /// map, then fails - the race of finding 1, made to happen on purpose.
    /// It can only be unblocked while the map is free, which is the point.
    struct WaitsToBeForgotten {
        writing: std::sync::mpsc::Sender<()>,
        forgotten: std::sync::mpsc::Receiver<()>,
    }

    impl std::io::Write for WaitsToBeForgotten {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let _ = self.writing.send(());
            let _ = self.forgotten.recv_timeout(Duration::from_secs(5));
            let _ = buf;
            Err(std::io::Error::other("the pipe went"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The whole of finding 1, through the real path: a pane that goes while
    /// a write to it is in flight is reported as a pane that has gone, not as
    /// an internal fault the caller is told to report as a bug.
    #[test]
    fn a_write_whose_pane_leaves_mid_write_says_the_pane_has_gone() {
        let supervisor = PtySupervisor::new(FlowLimits::default());
        let (sink, _out) = collecting_sink();
        supervisor.spawn(interactive("leaving"), sink).unwrap();

        let (writing, has_begun) = channel();
        let (forget, forgotten) = channel();
        {
            let mut panes = supervisor.panes.lock().unwrap();
            let pane = panes.get_mut("leaving").expect("the pane");
            pane.writer = super::hold(Box::new(WaitsToBeForgotten { writing, forgotten }));
        }

        let panes = supervisor.panes.clone();
        let taken_out = thread::spawn(move || {
            has_begun.recv_timeout(Duration::from_secs(5)).unwrap();
            // Only possible because the write is not holding the map.
            panes.lock().unwrap().remove("leaving");
            let _ = forget.send(());
        });

        let sent = supervisor.write("leaving", b"anyone there?");
        taken_out.join().unwrap();
        assert!(
            matches!(sent, Err(PtyError::NoSuchPane(_))),
            "a pane that went mid-write: {sent:?}"
        );
    }

    /// A pane that exits while a write to it is in flight is a race, not a
    /// fault: the caller is told the pane has gone, as it was when the map
    /// was held for the whole write. Anything else is the error it was.
    #[test]
    fn a_write_that_fails_because_its_pane_has_gone_says_so() {
        let supervisor = PtySupervisor::new(FlowLimits::default());
        let (sink, _out) = collecting_sink();
        supervisor.spawn(interactive("here"), sink).unwrap();
        let broke = || std::io::Error::other("the pipe went");

        let gone = super::failure(&supervisor.panes, "left-already", broke());
        assert!(matches!(gone, PtyError::NoSuchPane(_)), "{gone:?}");

        let here = super::failure(&supervisor.panes, "here", broke());
        assert!(matches!(here, PtyError::Io(_)), "{here:?}");
        supervisor.kill("here").ok();
    }
}
