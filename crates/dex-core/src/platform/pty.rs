//! PTY supervisor: spawns ConPTY children and owns their I/O (docs/prd.md §7.1).
//!
//! Each pane gets three named threads:
//! - **reader** — blocking reads into 64KB chunks (`portable-pty` blocks);
//! - **coalescer** — batches chunks (~8ms or 32KB) for the display, and applies
//!   watermark flow control (see `reader.rs`);
//! - **waiter** — waits for the child to exit, then closes the PTY.
//!
//! Shutdown: the child exits (or `kill` ends it); the waiter stores its code
//! and drops the master; the reader ends at EOF (on Unix as the child exits);
//! the coalescer flushes, waits for the code, reports `Exited`. Nothing joins.
//!
//! ConPTY's first output is a cursor-position query (`ESC[6n`, from
//! `PSEUDOCONSOLE_INHERIT_CURSOR`), and it renders nothing until answered.
//! xterm.js answers; any other driver (tests, a headless consumer) must too.

mod kill;
mod reader;
#[cfg(any(unix, test))]
mod reap;
mod relay;
mod session_env;
mod shell;
#[cfg(test)]
mod tests;
mod watch;

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
use relay::Relay;
use session_env::forget_claude_session;
pub use shell::{resolve_shell, shell_args};
pub use watch::Answer;

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
    /// Windows would not end the pane's process (`kill.rs`).
    #[error("Windows could not end the pane's process")]
    KillFailed,
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
    /// Where the output goes: the window showing the pane (`relay.rs`).
    relay: Arc<Relay>,
    /// The shell's process id, where the platform reports one.
    pid: Option<u32>,
}

/// The live panes, shared by every clone of the supervisor.
type Panes = Arc<Mutex<HashMap<String, Pane>>>;

/// Owns every live PTY. Cheap to clone; clones share the same panes.
#[derive(Clone)]
pub struct PtySupervisor {
    panes: Panes,
    limits: FlowLimits,
    watches: watch::Watches,
}

impl PtySupervisor {
    /// A supervisor with no panes.
    pub fn new(limits: FlowLimits) -> Self {
        Self {
            panes: Arc::new(Mutex::new(HashMap::new())),
            limits,
            watches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Types `answer` into a pane once it prints the phrase the answer waits
    /// for. At most one answer is armed per pane; a later one replaces it.
    ///
    /// The supervisor does not otherwise look at what a pane prints. This is
    /// for prompts a spawned agent would sit at forever, having no human to
    /// press the key — see `watch.rs`.
    pub fn answer_once(&self, pane_id: &str, answer: Answer) {
        watch::arm(&self.watches, pane_id, answer);
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
        let pid = child.process_id();
        let flow = Arc::new(Flow::default());
        let exit_code = Arc::new(reader::ExitSlot::default());
        let (chunks_tx, chunks_rx) = sync_channel(CHUNK_QUEUE);
        let id = request.pane_id.clone();

        spawn_named(format!("pty-reader-{id}"), move || {
            reader::run_reader(reader, chunks_tx);
        })?;
        let coalescer_flow = flow.clone();
        let coalescer_exit = exit_code.clone();
        let limits = self.limits;
        // The display's sink sits behind a relay, so a pane can move between
        // windows; the watch sees everything, wherever it goes.
        let relay = Arc::new(Relay::new(sink));
        let to_display = relay.clone();
        let sink = self.watching(&id, Box::new(move |output| to_display.deliver(output)));
        spawn_named(format!("pty-coalescer-{id}"), move || {
            reader::run_coalescer(chunks_rx, coalescer_flow, limits, sink, coalescer_exit);
        })?;

        self.lock().insert(
            id.clone(),
            Pane {
                master: pair.master,
                writer,
                killer,
                pid,
                flow,
                relay,
            },
        );

        let panes = self.panes.clone();
        let watches = self.watches.clone();
        spawn_named(format!("pty-waiter-{id}"), move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            watch::disarm(&watches, &id);
            // Out of the map before the code is out, so nothing reaches a pane
            // already reported gone. Dropped (closing the pseudoconsole) after
            // the lock: ClosePseudoConsole can block until the reader drains,
            // which flow control may be pausing - that would stall every pane.
            let pane = panes.lock().ok().and_then(|mut map| map.remove(&id));
            exit_code.set(code);
            drop(pane);
            tracing::debug!(pane = %id, ?code, "pty child exited");
        })?;
        Ok(())
    }

    /// Sends input bytes to a pane's process.
    pub fn write(&self, pane_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
        write_to(&self.panes, pane_id, bytes)
    }

    /// Wraps a pane's sink so an armed answer sees the output on its way past.
    /// The display's sink still receives everything, unchanged and in order.
    fn watching(&self, pane_id: &str, mut sink: OutputSink) -> OutputSink {
        let watches = self.watches.clone();
        let panes = self.panes.clone();
        let pane_id = pane_id.to_owned();
        Box::new(move |output| {
            if let PtyOutput::Data(bytes) = &output {
                watch::observe(&watches, &panes, &pane_id, bytes);
            }
            sink(output);
        })
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

    /// When a pane's output last reached the display, unix millis (0 if never);
    /// `None` if the pane has no live process.
    pub fn last_output_at(&self, pane_id: &str) -> Option<i64> {
        self.lock().get(pane_id).map(|pane| pane.flow.last_output())
    }

    /// The pid of a pane's shell, the root of all that runs in it, if alive.
    pub fn shell_pid(&self, pane_id: &str) -> Option<u32> {
        self.lock().get(pane_id).and_then(|pane| pane.pid)
    }

    /// How many panes have a live process, for "quit and end them all?".
    pub fn live(&self) -> usize {
        self.lock().len()
    }

    /// Keeps a pane's output instead of sending it to its window, for a move
    /// between windows. Returns the bytes that window has been sent since it
    /// attached, so it can wait for all of them before serializing.
    pub fn hold(&self, pane_id: &str) -> Result<u64, PtyError> {
        Ok(self.relay(pane_id)?.hold())
    }

    /// Sends a pane's output to `sink` from now on, starting with whatever
    /// was kept since `hold`.
    pub fn attach(&self, pane_id: &str, sink: OutputSink) -> Result<(), PtyError> {
        self.relay(pane_id)?.attach(sink);
        Ok(())
    }

    fn relay(&self, pane_id: &str) -> Result<Arc<Relay>, PtyError> {
        self.lock()
            .get(pane_id)
            .map(|pane| pane.relay.clone())
            .ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))
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
    /// On macOS and Linux everything under the shell goes too (`reap.rs`).
    pub fn kill(&self, pane_id: &str) -> Result<(), PtyError> {
        let mut panes = self.lock();
        let pane = panes
            .get_mut(pane_id)
            .ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
        #[cfg(unix)]
        if let Some(pid) = pane.pid {
            spawn_named(format!("pty-reap-{pane_id}"), move || {
                reap::end_tree(pid, reap::CLOSE_GRACE);
            })?;
            return Ok(());
        }
        kill::outcome(pane.killer.kill())
    }

    /// Ends every pane's processes as the app quits (Windows' job already does).
    pub fn end_all(&self) {
        #[cfg(unix)]
        reap::end_all(self.lock().values().filter_map(|pane| pane.pid).collect());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pane>> {
        // A poisoned lock means a thread panicked mid-update; the map itself
        // is still usable, and refusing all PTY I/O would be worse.
        self.panes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Sends input to a pane, given the map directly. `watch` answers prompts from
/// its own thread and has no supervisor to call.
fn write_to(panes: &Panes, pane_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
    let mut panes = panes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let pane = panes
        .get_mut(pane_id)
        .ok_or_else(|| PtyError::NoSuchPane(pane_id.to_owned()))?;
    pane.writer.write_all(bytes)?;
    pane.writer.flush()?;
    Ok(())
}

fn command(request: &SpawnRequest) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(&request.program);
    cmd.args(&request.args);
    cmd.cwd(&request.cwd);
    forget_claude_session(&mut cmd);
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
