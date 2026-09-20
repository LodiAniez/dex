//! Typing into a pane, without holding every other pane while it happens.
//!
//! A PTY write blocks when the child is not reading its input: its buffer
//! fills, and `write_all` waits for room. Shells and Claude Code drain their
//! input constantly, so this is rare - but the map of live panes is shared by
//! the whole daemon, and a write that held it would stop every other pane
//! being typed into, resized, opened, killed, or asked when it last spoke
//! (the watchdog asks that of every running agent, every sweep). One stuck
//! pane would look like a frozen app (issue #63).
//!
//! So each pane's writer has a lock of its own. The map is held only long
//! enough to take a handle to it: writes to one pane still take turns, which
//! is what keeps text and the Enter after it in order, and writes to
//! different panes no longer wait on each other.

use std::io::Write;
use std::sync::{Arc, Mutex};

use super::{Panes, PtyError};

/// A pane's end of the PTY, behind its own lock.
pub(super) type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// Wraps a freshly opened PTY's writer for the pane to hold.
pub(super) fn writer(writer: Box<dyn Write + Send>) -> Writer {
    Arc::new(Mutex::new(writer))
}

/// A handle to the pane's writer, with the map released before the caller
/// writes anything through it.
pub(super) fn writer_of(panes: &Panes, pane_id: &str) -> Option<Writer> {
    let panes = panes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    panes.get(pane_id).map(|pane| pane.writer.clone())
}

/// Sends input to a pane, given the map directly. `watch` answers prompts from
/// its own thread and has no supervisor to call.
pub(super) fn write_to(panes: &Panes, pane_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
    let writer =
        writer_of(panes, pane_id).ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
    let mut writer = writer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    writer.write_all(bytes)?;
    writer.flush()?;
    Ok(())
}
