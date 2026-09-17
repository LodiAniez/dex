//! PTY supervisor tests against real ConPTY children.

use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use super::*;

/// ConPTY's opening cursor-position query, and a terminal's answer to it.
const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_REPLY: &[u8] = b"\x1b[1;1R";

fn collecting_sink() -> (OutputSink, Receiver<PtyOutput>) {
    let (tx, rx) = channel();
    let sink: OutputSink = Box::new(move |out| {
        let _ = tx.send(out);
    });
    (sink, rx)
}

fn cmd(pane_id: &str, script: &str) -> SpawnRequest {
    SpawnRequest {
        pane_id: pane_id.to_owned(),
        program: PathBuf::from("cmd.exe"),
        args: vec!["/c".into(), script.into()],
        cwd: std::env::temp_dir(),
        env: vec![("DEX_PANE_ID".into(), pane_id.into())],
        cols: 120,
        rows: 30,
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// What a test saw on a pane until it exited or the timeout hit.
struct Drained {
    bytes: Vec<u8>,
    events: Vec<PtyOutput>,
}

impl Drained {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    fn exited(&self) -> bool {
        matches!(self.events.last(), Some(PtyOutput::Exited { .. }))
    }
}

/// Plays the terminal: answers ConPTY's cursor query (as xterm.js does in
/// the app — ConPTY renders nothing until it is answered) and, if `ack` is
/// set, acknowledges output. Stops at `Exited` or the timeout.
fn drain(
    rx: &Receiver<PtyOutput>,
    supervisor: &PtySupervisor,
    pane: &str,
    ack: bool,
    timeout: Duration,
) -> Drained {
    let deadline = Instant::now() + timeout;
    let mut seen = Drained {
        bytes: Vec::new(),
        events: Vec::new(),
    };
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(PtyOutput::Data(data)) => {
                if contains(&data, CURSOR_QUERY) {
                    let _ = supervisor.write(pane, CURSOR_REPLY);
                }
                if ack {
                    let _ = supervisor.ack(pane, data.len());
                }
                seen.bytes.extend(data);
            }
            Ok(other) => {
                let done = matches!(other, PtyOutput::Exited { .. });
                seen.events.push(other);
                if done {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    seen
}

#[test]
fn child_output_reaches_the_sink_and_exit_is_reported() {
    let supervisor = PtySupervisor::new(FlowLimits::default());
    let (sink, rx) = collecting_sink();
    supervisor
        .spawn(cmd("p1", "echo hello-from-dex & exit /b 3"), sink)
        .unwrap();

    let seen = drain(&rx, &supervisor, "p1", true, Duration::from_secs(15));

    assert!(
        seen.text().contains("hello-from-dex"),
        "output {:?}, events {:?}",
        seen.text(),
        seen.events
    );
    assert_eq!(
        seen.events.last(),
        Some(&PtyOutput::Exited { code: Some(3) })
    );
    assert!(matches!(
        supervisor.write("p1", b"x"),
        Err(PtyError::NoSuchPane(_))
    ));
}

#[test]
fn spawning_a_duplicate_pane_id_is_rejected() {
    let supervisor = PtySupervisor::new(FlowLimits::default());
    let (sink, rx) = collecting_sink();
    supervisor
        .spawn(cmd("dup", "ping -n 30 127.0.0.1 >nul"), sink)
        .unwrap();
    let (sink2, _rx2) = collecting_sink();
    let second = supervisor.spawn(cmd("dup", "echo no"), sink2);
    assert!(
        matches!(second, Err(PtyError::AlreadyExists(_))),
        "{second:?}"
    );

    supervisor.kill("dup").unwrap();
    let seen = drain(&rx, &supervisor, "dup", true, Duration::from_secs(15));
    assert!(
        seen.exited(),
        "killed child must report exit: {:?}",
        seen.events
    );
}

#[test]
fn output_pauses_while_the_display_is_not_acknowledging() {
    let limits = FlowLimits {
        high: 4 * 1024,
        low: 1024,
        stall: Duration::from_secs(60),
    };
    let supervisor = PtySupervisor::new(limits);
    let (sink, rx) = collecting_sink();
    supervisor
        .spawn(
            cmd(
                "slow",
                "for /L %i in (1,1,50000) do @echo flow control line %i",
            ),
            sink,
        )
        .unwrap();

    // Never acknowledge: the coalescer must stop after at most one batch past `high`.
    let paused = drain(&rx, &supervisor, "slow", false, Duration::from_secs(3));
    assert!(
        paused.events.is_empty(),
        "child must still be paused, got {:?}",
        paused.events
    );
    assert!(
        paused.bytes.len() > limits.high,
        "output never started: {:?}",
        paused.text()
    );
    assert!(
        paused.bytes.len() <= limits.high + 2 * 32 * 1024,
        "sent {} bytes without acknowledgement",
        paused.bytes.len()
    );

    // Acknowledge what arrived, and everything after: the child runs to completion.
    supervisor.ack("slow", paused.bytes.len()).unwrap();
    let rest = drain(&rx, &supervisor, "slow", true, Duration::from_secs(120));
    assert!(rest.exited(), "events {:?}", rest.events);
}

#[test]
fn an_unresponsive_display_cannot_wedge_the_child() {
    let limits = FlowLimits {
        high: 4 * 1024,
        low: 1024,
        stall: Duration::from_millis(300),
    };
    let supervisor = PtySupervisor::new(limits);
    let (sink, rx) = collecting_sink();
    supervisor
        .spawn(
            cmd(
                "stuck",
                "for /L %i in (1,1,20000) do @echo discarded line %i",
            ),
            sink,
        )
        .unwrap();

    // Never acknowledge anything. The stall must trigger discarding, so the
    // child still finishes, and the sink is told output was dropped.
    let seen = drain(&rx, &supervisor, "stuck", false, Duration::from_secs(120));
    assert!(
        seen.events
            .iter()
            .any(|e| matches!(e, PtyOutput::Dropped { bytes } if *bytes > 0)),
        "expected a Dropped event, got {:?}",
        seen.events
    );
    assert!(seen.exited(), "events {:?}", seen.events);
}

/// Backend half of the M1 throughput gate: `type` a 50MB file through ConPTY
/// with instant acknowledgement.
/// Run with `cargo test -p dex-core --release -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark; takes tens of seconds"]
fn throughput_type_50mb_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("big.txt");
    let line =
        "The quick brown fox jumps over the lazy dog 0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ\r\n";
    let mut content = String::with_capacity(50 * 1024 * 1024 + line.len());
    while content.len() < 50 * 1024 * 1024 {
        content.push_str(line);
    }
    std::fs::write(&file, &content).unwrap();

    let supervisor = PtySupervisor::new(FlowLimits::default());
    let (sink, rx) = collecting_sink();
    let started = Instant::now();
    supervisor
        .spawn(
            SpawnRequest {
                // Separate args: portable-pty escapes embedded quotes as \", which cmd rejects.
                args: vec!["/c".into(), "type".into(), file.display().to_string()],
                ..cmd("bench", "")
            },
            sink,
        )
        .unwrap();
    let seen = drain(&rx, &supervisor, "bench", true, Duration::from_secs(600));
    let elapsed = started.elapsed();

    // Cut the tail from the bytes: slicing a String could split a UTF-8 character.
    let tail = String::from_utf8_lossy(&seen.bytes[seen.bytes.len().saturating_sub(300)..]);
    assert_eq!(
        seen.events.last(),
        Some(&PtyOutput::Exited { code: Some(0) }),
        "tail of output: {tail:?}"
    );
    assert!(seen.bytes.len() > 1024 * 1024, "tail of output: {tail:?}");
    println!(
        "typed {} MB of input; ConPTY emitted {} MB in {:.1}s",
        content.len() / (1024 * 1024),
        seen.bytes.len() / (1024 * 1024),
        elapsed.as_secs_f64()
    );
}

#[test]
fn a_pane_does_not_inherit_the_identity_of_a_claude_session_dex_was_started_from() {
    // Dex started from a terminal inside Claude Code inherits that session's
    // variables, and every `claude` in a pane then believes it is its child:
    // transcripts off, and messages routed to a socket that is not its own.
    let mut cmd = portable_pty::CommandBuilder::new("cmd.exe");
    for name in [
        "CLAUDECODE",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "CLAUDE_PID",
    ] {
        cmd.env(name, "inherited");
    }
    // The owner's own settings for Claude Code are theirs, and must get through.
    cmd.env("CLAUDE_CONFIG_DIR", "D:/claude");
    cmd.env("ANTHROPIC_MODEL", "opus");

    super::forget_claude_session(&mut cmd);

    for name in [
        "CLAUDECODE",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "CLAUDE_PID",
    ] {
        assert!(cmd.get_env(name).is_none(), "{name} reached the pane");
    }
    assert!(cmd.get_env("CLAUDE_CONFIG_DIR").is_some());
    assert!(cmd.get_env("ANTHROPIC_MODEL").is_some());
}

#[test]
fn what_dex_sets_for_a_pane_is_set_after_the_forgetting() {
    let request = SpawnRequest {
        env: vec![("CLAUDE_CODE_SESSION_ID".into(), "asked-for".into())],
        ..cmd("forget-order", "exit 0")
    };
    let built = super::command(&request);
    assert!(
        built.get_env("CLAUDE_CODE_SESSION_ID").is_some(),
        "a caller that sets one on purpose is obeyed"
    );
}
