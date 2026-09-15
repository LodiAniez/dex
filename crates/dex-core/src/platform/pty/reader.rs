//! A pane's output path: the blocking reader thread, the coalescer thread, and
//! the flow-control state shared with acknowledgements.
//!
//! Flow control works through the bounded chunk queue. While the display is
//! more than `high` bytes behind, the coalescer stops pulling chunks; the queue
//! fills; the reader blocks on send; it stops reading the PTY; ConPTY's buffer
//! fills; and the child blocks on write. Nothing is ever dropped from the
//! middle of the stream — that would split escape sequences and corrupt the
//! display (PRD §7.1). The one exception is a display that stops acknowledging
//! entirely for `stall`: then output is discarded so the child is not wedged
//! forever, and the sink is told so it can reset the terminal.

use std::io::{ErrorKind, Read};
use std::mem;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::{FlowLimits, OutputSink, PtyOutput};

/// Size of each read from the PTY.
const READ_BYTES: usize = 64 * 1024;
/// Flush a batch once it reaches this size...
const BATCH_BYTES: usize = 32 * 1024;
/// ...or once its first chunk is this old, whichever comes first.
const BATCH_WINDOW: Duration = Duration::from_millis(8);

/// Unacknowledged-output accounting, shared by the coalescer and `ack`.
#[derive(Debug, Default)]
pub(super) struct Flow {
    state: Mutex<FlowState>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct FlowState {
    /// Bytes sent to the sink and not yet acknowledged.
    unacked: usize,
    /// Count of acknowledgements ever received; a change means the display is alive.
    acks: u64,
}

/// Whether the coalescer may keep sending.
#[derive(Debug, PartialEq, Eq)]
enum Room {
    Available,
    Stalled,
}

impl Flow {
    /// Records that the display processed `bytes`.
    pub(super) fn ack(&self, bytes: usize) {
        let mut state = self.lock();
        // Late acks for output sent before a drop can exceed the count; saturate.
        state.unacked = state.unacked.saturating_sub(bytes);
        state.acks += 1;
        self.changed.notify_all();
    }

    fn sent(&self, bytes: usize) {
        self.lock().unacked += bytes;
    }

    fn acks(&self) -> u64 {
        self.lock().acks
    }

    /// Returns at once if the backlog is at most `high`. Otherwise blocks until
    /// it falls below `low`, or reports a stall if no ack arrives in time.
    fn wait_for_room(&self, limits: FlowLimits) -> Room {
        let mut state = self.lock();
        if state.unacked <= limits.high {
            return Room::Available;
        }
        let mut last_acks = state.acks;
        let mut quiet_since = Instant::now();
        while state.unacked >= limits.low {
            // The stall clock restarts on every ack: a slow display is not a dead one.
            if state.acks != last_acks {
                last_acks = state.acks;
                quiet_since = Instant::now();
            }
            let Some(remaining) = limits.stall.checked_sub(quiet_since.elapsed()) else {
                return Room::Stalled;
            };
            state = match self.changed.wait_timeout(state, remaining) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
        Room::Available
    }

    /// Forgets the backlog after a drop; the display will be reset anyway.
    fn clear(&self) {
        self.lock().unacked = 0;
    }

    fn lock(&self) -> MutexGuard<'_, FlowState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Reads the PTY until EOF or until the coalescer is gone.
pub(super) fn run_reader(mut reader: Box<dyn Read + Send>, chunks: SyncSender<Vec<u8>>) {
    let mut buf = vec![0u8; READ_BYTES];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                // Blocks while the queue is full: this is the pause in flow control.
                if chunks.send(buf[..n].to_vec()).is_err() {
                    return;
                }
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(err) => {
                // Expected when the pseudoconsole closes under a pending read.
                tracing::debug!(%err, "pty read ended");
                return;
            }
        }
    }
}

/// Batches chunks for the sink and enforces flow control, until the reader ends.
pub(super) fn run_coalescer(
    chunks: Receiver<Vec<u8>>,
    flow: Arc<Flow>,
    limits: FlowLimits,
    mut sink: OutputSink,
    exit_code: Arc<Mutex<Option<u32>>>,
) {
    let mut batch = Batch::default();
    loop {
        if flow.wait_for_room(limits) == Room::Stalled {
            let dropped = discard_until_display_returns(&chunks, &flow);
            flow.clear();
            batch.clear();
            sink(PtyOutput::Dropped {
                bytes: dropped.bytes,
            });
            if dropped.reader_ended {
                break;
            }
            continue;
        }

        let next = match batch.deadline {
            None => chunks.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(deadline) => {
                chunks.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }
        };
        match next {
            Ok(chunk) => {
                batch.push(chunk);
                if batch.bytes.len() >= BATCH_BYTES {
                    batch.flush(&flow, &mut sink);
                }
            }
            Err(RecvTimeoutError::Timeout) => batch.flush(&flow, &mut sink),
            Err(RecvTimeoutError::Disconnected) => {
                batch.flush(&flow, &mut sink);
                break;
            }
        }
    }
    let code = exit_code.lock().ok().and_then(|slot| *slot);
    sink(PtyOutput::Exited { code });
}

struct Discarded {
    bytes: u64,
    reader_ended: bool,
}

/// Keeps the PTY flowing into the void until the display acknowledges
/// anything again (or the reader ends), so an unresponsive UI cannot wedge the
/// child process.
fn discard_until_display_returns(chunks: &Receiver<Vec<u8>>, flow: &Flow) -> Discarded {
    tracing::warn!("display stopped acknowledging output; discarding until it recovers");
    let acks_at_stall = flow.acks();
    let mut bytes = 0u64;
    loop {
        if flow.acks() != acks_at_stall {
            return Discarded {
                bytes,
                reader_ended: false,
            };
        }
        match chunks.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => bytes += chunk.len() as u64,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Discarded {
                    bytes,
                    reader_ended: true,
                };
            }
        }
    }
}

/// Output waiting to be flushed, and when it must go at the latest.
#[derive(Default)]
struct Batch {
    bytes: Vec<u8>,
    deadline: Option<Instant>,
}

impl Batch {
    fn push(&mut self, chunk: Vec<u8>) {
        if self.bytes.is_empty() {
            self.deadline = Some(Instant::now() + BATCH_WINDOW);
        }
        self.bytes.extend_from_slice(&chunk);
    }

    fn flush(&mut self, flow: &Flow, sink: &mut OutputSink) {
        if self.bytes.is_empty() {
            return;
        }
        let data = mem::take(&mut self.bytes);
        self.deadline = None;
        flow.sent(data.len());
        sink(PtyOutput::Data(data));
    }

    fn clear(&mut self) {
        self.bytes.clear();
        self.deadline = None;
    }
}
