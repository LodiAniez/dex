//! PTY supervisor: spawns ConPTY children and owns their I/O (docs/prd.md §7.1).
//!
//! Each pane gets three named threads:
//! - **reader** — blocking reads from the PTY into 64KB chunks (`portable-pty`
//!   readers are blocking, so this is a thread, not an async task);
//! - **coalescer** — batches chunks (~8ms or 32KB) for the display, and applies
//!   watermark flow control (see `reader.rs`);
//! - **waiter** — waits for the child to exit, then closes the PTY.
//!
//! Shutdown path: the child exits (or `kill` makes it exit) → the waiter drops
//! the master, closing the pseudoconsole → the reader sees EOF and ends → the
//! coalescer flushes, reports `Exited`, and ends. Nothing needs joining.
//!
//! ConPTY behavior to know about: `portable-pty` creates the pseudoconsole
//! with `PSEUDOCONSOLE_INHERIT_CURSOR`, so ConPTY's first output is a
//! cursor-position query (`ESC[6n`) and it renders *nothing* until the
//! terminal answers. xterm.js answers automatically in the app; anything else
//! driving a pane (tests, a future headless consumer) must answer it too, or
//! the pane looks dead.

mod reader;
mod shell;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

use reader::Flow;
pub use shell::resolve_shell;

/// How many reader chunks may queue before the reader blocks. Small on
/// purpose: once the display is behind, the PTY should pause promptly.
const CHUNK_QUEUE: usize = 4;

/// Watermarks for display flow control, in bytes of unacknowledged output.
#[derive(Debug, Clone, Copy)]
pub struct FlowLimits {
    /// Stop pulling output once this many bytes await acknowledgement.
    pub high: usize,
    /// Resume once acknowledgements bring the backlog below this.
    pub low: usize,
    /// If no acknowledgement arrives for this long while paused, the display
    /// is considered unresponsive and output is discarded instead.
    pub stall: Duration,
}

impl Default for FlowLimits {
    /// PRD §7.1 defaults; xterm.js advises keeping `high` at or below ~500KB
    /// so keystrokes stay snappy under heavy output.
    fn default() -> Self {
        Self {
            high: 512 * 1024,
            low: 128 * 1024,
            stall: Duration::from_secs(5),
        }
    }
}

/// What a pane's output sink receives, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyOutput {
    /// Coalesced terminal output, to be written to the display and acknowledged.
    Data(Vec<u8>),
    /// Output was discarded while the display was unresponsive. The display
    /// should reset the terminal and show a marker.
    Dropped {
        /// How many bytes were discarded.
        bytes: u64,
    },
    /// The child exited; nothing follows.
    Exited {
        /// Exit code, if the child reported one.
        code: Option<u32>,
    },
}

/// Receives a pane's output. Called from that pane's coalescer thread.
pub type OutputSink = Box<dyn FnMut(PtyOutput) + Send>;

/// Everything needed to start a pane's process.
#[derive(Debug, Clone)]
pub struct SpawnRequest {
    /// Pane id; also exported to the child as `DEX_PANE_ID` by the caller's `env`.
    pub pane_id: String,
    /// Executable to run.
    pub program: PathBuf,
    /// Arguments.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
    /// Initial size in character cells.
    pub cols: u16,
    /// Initial size in character cells.
    pub rows: u16,
}

/// Failures from PTY operations.
#[derive(Debug, Error)]
pub enum PtyError {
    /// No live pane has this id (it never existed or its process has exited).
    #[error("no live pane {0}")]
    NoSuchPane(String),
    /// A live pane already has this id.
    #[error("pane {0} already exists")]
    AlreadyExists(String),
    /// ConPTY or process creation failed. `portable-pty` reports errors as
    /// `anyhow`, which a library must not re-export, so they arrive as text.
    #[error("pty: {0}")]
    Pty(String),
    /// Writing to the PTY or starting a thread failed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// A live pane's handles. Removed from the map when its child exits.
struct Pane {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    flow: Arc<Flow>,
}

/// Owns every live PTY. Cheap to clone; clones share the same panes.
#[derive(Clone)]
pub struct PtySupervisor {
    panes: Arc<Mutex<HashMap<String, Pane>>>,
    limits: FlowLimits,
}

impl PtySupervisor {
    /// A supervisor with no panes.
    pub fn new(limits: FlowLimits) -> Self {
        Self {
            panes: Arc::new(Mutex::new(HashMap::new())),
            limits,
        }
    }

    /// Starts `request.program` in a new PTY; its output goes to `sink`.
    pub fn spawn(&self, request: SpawnRequest, sink: OutputSink) -> Result<(), PtyError> {
        if self.lock().contains_key(&request.pane_id) {
            return Err(PtyError::AlreadyExists(request.pane_id));
        }

        let pair = native_pty_system()
            .openpty(cell_size(request.cols, request.rows))
            .map_err(pty_err)?;
        let mut child = pair
            .slave
            .spawn_command(command(&request))
            .map_err(pty_err)?;
        // The child holds its own copy of the slave side; ours must be closed
        // or the pseudoconsole never reports EOF.
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().map_err(pty_err)?;
        let writer = pair.master.take_writer().map_err(pty_err)?;
        let killer = child.clone_killer();
        let flow = Arc::new(Flow::default());
        let exit_code = Arc::new(Mutex::new(None));
        let (chunks_tx, chunks_rx) = sync_channel(CHUNK_QUEUE);
        let id = request.pane_id.clone();

        spawn_named(format!("pty-reader-{id}"), move || {
            reader::run_reader(reader, chunks_tx);
        })?;
        let coalescer_flow = flow.clone();
        let coalescer_exit = exit_code.clone();
        let limits = self.limits;
        spawn_named(format!("pty-coalescer-{id}"), move || {
            reader::run_coalescer(chunks_rx, coalescer_flow, limits, sink, coalescer_exit);
        })?;

        self.lock().insert(
            id.clone(),
            Pane {
                master: pair.master,
                writer,
                killer,
                flow,
            },
        );

        let panes = self.panes.clone();
        spawn_named(format!("pty-waiter-{id}"), move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            if let Ok(mut slot) = exit_code.lock() {
                *slot = code;
            }
            // Take the pane out under the lock, but drop it (closing the
            // pseudoconsole) after releasing it: ClosePseudoConsole can block
            // until the reader drains, and the reader may be paused by flow
            // control — holding the lock here would stall every other pane.
            let pane = panes.lock().ok().and_then(|mut map| map.remove(&id));
            drop(pane);
            tracing::debug!(pane = %id, ?code, "pty child exited");
        })?;
        Ok(())
    }

    /// Sends input bytes to a pane's process.
    pub fn write(&self, pane_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
        let mut panes = self.lock();
        let pane = panes
            .get_mut(pane_id)
            .ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
        pane.writer.write_all(bytes)?;
        pane.writer.flush()?;
        Ok(())
    }

    /// Resizes a pane. Callers debounce (PRD §7.1: 100ms), since ConPTY
    /// repaints on every resize.
    pub fn resize(&self, pane_id: &str, cols: u16, rows: u16) -> Result<(), PtyError> {
        let panes = self.lock();
        let pane = panes
            .get(pane_id)
            .ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
        pane.master.resize(cell_size(cols, rows)).map_err(pty_err)
    }

    /// Records that the display has processed `bytes` of a pane's output.
    pub fn ack(&self, pane_id: &str, bytes: usize) -> Result<(), PtyError> {
        let flow = self
            .lock()
            .get(pane_id)
            .map(|pane| pane.flow.clone())
            .ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
        flow.ack(bytes);
        Ok(())
    }

    /// Kills a pane's process. Its final output and `Exited` still arrive.
    pub fn kill(&self, pane_id: &str) -> Result<(), PtyError> {
        let mut panes = self.lock();
        let pane = panes
            .get_mut(pane_id)
            .ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
        match pane.killer.kill() {
            // portable-pty 0.9.0's cloned Windows killer has its check inverted:
            // it returns `last_os_error()` when TerminateProcess *succeeds*,
            // which is always OS error 0. Treat that as the success it is.
            // (A genuine failure comes back as Ok and cannot be detected.)
            Err(err) if err.raw_os_error() == Some(0) => Ok(()),
            result => result.map_err(PtyError::from),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pane>> {
        // A poisoned lock means a thread panicked mid-update; the map itself
        // is still usable, and refusing all PTY I/O would be worse.
        self.panes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn command(request: &SpawnRequest) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(&request.program);
    cmd.args(&request.args);
    cmd.cwd(&request.cwd);
    for (key, value) in &request.env {
        cmd.env(key, value);
    }
    cmd
}

fn cell_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn pty_err(err: impl std::fmt::Display) -> PtyError {
    PtyError::Pty(err.to_string())
}

fn spawn_named(name: String, body: impl FnOnce() + Send + 'static) -> Result<(), PtyError> {
    thread::Builder::new().name(name).spawn(body)?;
    Ok(())
}
