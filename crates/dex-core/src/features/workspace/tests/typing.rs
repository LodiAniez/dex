//! Typing into a pane does not go through the database (issue #86).
//!
//! It used to, on every character: `resolve_pane` reads every pane row through
//! Dex's single connection, so each keystroke queued behind every hook from
//! every agent and every query the sweep makes. With several agents hooking at
//! each prompt and tool batch, a pane took no input for a minute and then took
//! all of it at once.
//!
//! The test for that is not a timing: it is a pane the supervisor holds and
//! the database has never heard of. If a write to it succeeds, the lookup was
//! not consulted - there is nothing for it to have found.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use dex_protocol::pane::{LabelPaneArgs, SendArgs, SendKeyArgs};
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::super::model::WorkspaceError;
use super::super::pane_io::{ENTER_WAIT, took_the_text};
use super::super::{send, send_key};
use crate::app::AppState;
use crate::features::workspace;
use crate::platform::pty::{OutputSink, PtyOutput, SpawnRequest};

/// A live pane that exists only in the supervisor: no workspace, no row, no
/// label - which is not how Dex makes panes, and is exactly what makes this
/// test say something.
fn only_in_the_supervisor(state: &AppState, pane: &str) {
    let program = if cfg!(windows) {
        PathBuf::from("cmd.exe")
    } else {
        PathBuf::from("/bin/sh")
    };
    state
        .pty
        .spawn(
            SpawnRequest {
                pane_id: pane.to_owned(),
                program,
                args: Vec::new(),
                cwd: std::env::temp_dir(),
                env: vec![("DEX_PANE_ID".to_owned(), pane.to_owned())],
                cols: 80,
                rows: 24,
            },
            Box::new(|_| {}),
        )
        .unwrap();
}

#[tokio::test]
async fn typing_reaches_a_pane_the_database_has_never_heard_of() {
    let (_dir, state) = AppState::for_tests();
    let pane = "a-pane-with-no-row";
    only_in_the_supervisor(&state, pane);

    let sent = send(
        &state,
        SendArgs {
            pane: pane.to_owned(),
            text: "echo hello".to_owned(),
            enter: false,
        },
    )
    .await
    .expect("the supervisor holds this pane, so no lookup is needed");

    assert_eq!(sent.pane, pane);
    state.pty.kill(pane).unwrap();
}

#[tokio::test]
async fn a_key_press_takes_the_same_path() {
    let (_dir, state) = AppState::for_tests();
    let pane = "another-pane-with-no-row";
    only_in_the_supervisor(&state, pane);

    let sent = send_key(
        &state,
        SendKeyArgs {
            pane: pane.to_owned(),
            key: dex_protocol::pane::Key::Enter,
        },
    )
    .await
    .expect("send_key writes through the same path");

    assert_eq!(sent.pane, pane);
    state.pty.kill(pane).unwrap();
}

#[tokio::test]
async fn a_pane_named_by_its_label_is_still_looked_up() {
    // What the lookup exists for: `dex pane send --pane reviewer`. The label
    // is in no supervisor map, so only the database can turn it into a pane -
    // and this is the test that fails if the fast path swallows everything.
    let (_dir, state) = AppState::for_tests();
    let list = workspace::create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let pane = list.workspaces[0].panes[0].id.clone();
    workspace::label_pane(
        &state,
        LabelPaneArgs {
            pane: pane.clone(),
            label: Some("reviewer".to_owned()),
        },
    )
    .await
    .unwrap();
    only_in_the_supervisor(&state, &pane);

    let sent = send(
        &state,
        SendArgs {
            pane: "reviewer".to_owned(),
            text: "hello".to_owned(),
            enter: false,
        },
    )
    .await
    .expect("a label reaches its pane through the lookup");

    assert_eq!(sent.pane, pane, "the label resolved to the pane's id");
    state.pty.kill(&pane).unwrap();
}

#[tokio::test]
async fn a_target_that_is_nothing_at_all_says_so_as_a_missing_pane() {
    // And it must be the lookup saying it: without one, the write itself
    // fails and the error is "the pane has not started", which is a different
    // thing and sends the reader somewhere else.
    let (_dir, state) = AppState::for_tests();

    let refused = send(
        &state,
        SendArgs {
            pane: "no-such-pane-anywhere".to_owned(),
            text: "hello".to_owned(),
            enter: false,
        },
    )
    .await;

    assert!(
        matches!(refused, Err(WorkspaceError::NoSuchPane(ref target)) if target == "no-such-pane-anywhere"),
        "the lookup refused it, rather than the write: {refused:?}"
    );
}

/// A pane running a child that says nothing and reads nothing.
///
/// Alive for far longer than the tests take, so a loaded machine cannot have
/// the child exit mid-test and leave them measuring a pane that has gone
/// (issue #97); it is killed at the end of each.
fn a_silent_pane(state: &AppState, pane: &str, sink: OutputSink) {
    let (program, args) = if cfg!(windows) {
        // `ping` rather than `timeout`, which prints a countdown.
        (
            "cmd.exe",
            vec!["/c".to_owned(), "ping -n 60 127.0.0.1 >nul".to_owned()],
        )
    } else {
        ("/bin/sh", vec!["-c".to_owned(), "sleep 60".to_owned()])
    };
    state
        .pty
        .spawn(
            SpawnRequest {
                pane_id: pane.to_owned(),
                program: PathBuf::from(program),
                args,
                cwd: std::env::temp_dir(),
                env: vec![("DEX_PANE_ID".to_owned(), pane.to_owned())],
                cols: 80,
                rows: 24,
            },
            sink,
        )
        .unwrap();
}

/// Waits until the pane has gone quiet, and says what it last said. A shell
/// has a banner and a prompt to print, and ConPTY an opening handshake.
async fn once_it_is_quiet(state: &AppState, pane: &str) -> Option<i64> {
    let give_up = Instant::now() + Duration::from_secs(5);
    let mut last = state.pty.last_output_at(pane);
    loop {
        tokio::time::sleep(Duration::from_millis(120)).await;
        let now = state.pty.last_output_at(pane);
        if now == last || Instant::now() > give_up {
            return now;
        }
        last = now;
    }
}

/// The Enter goes as soon as the pane has drawn, not on a timer.
#[tokio::test]
async fn the_enter_goes_once_the_pane_has_said_something() {
    let looks = std::cell::Cell::new(0);
    let said = || {
        looks.set(looks.get() + 1);
        if looks.get() < 3 { Some(10) } else { Some(11) }
    };

    let took = took_the_text(said, Some(10), Duration::from_secs(5)).await;

    assert!(took, "the pane spoke, so the text was read");
    assert_eq!(looks.get(), 3, "and it stopped looking the moment it had");
}

/// A pane that never says anything still gets its Enter: that is what every
/// send did before this, and a pane held up for ever would be worse.
#[tokio::test]
async fn a_pane_that_says_nothing_gets_its_enter_anyway() {
    let said = || Some(10);
    let waited = Instant::now();

    let took = took_the_text(said, Some(10), Duration::from_millis(60)).await;

    assert!(!took, "nothing was ever heard from it");
    assert!(
        waited.elapsed() >= Duration::from_millis(60),
        "and it waited to be sure: {:?}",
        waited.elapsed()
    );
}

/// The wiring: a real pane that says nothing really does hold the Enter back.
///
/// Timed, because the write itself is the same either way - what changed is
/// when it happens. A pane whose child neither echoes nor prints gives the
/// wait nothing to go on, so `send` returns only once it has given up.
///
/// Windows only, and that is the whole shape of the hazard: a Windows console
/// echoes as the child *reads*, so a child that is not reading leaves the pane
/// silent. A Unix pty's line discipline echoes on the child's behalf whether
/// it reads or not, so there is no such thing there as a pane that stays quiet
/// while being typed at - CI said so, which is what the premise below is for.
#[cfg(windows)]
#[tokio::test]
async fn a_silent_pane_makes_the_enter_wait_for_it() {
    let (_dir, state) = AppState::for_tests();
    let pane = "a-pane-with-nothing-to-say";
    a_silent_pane(&state, pane, Box::new(|_| {}));
    let quiet = once_it_is_quiet(&state, pane).await;

    let started = Instant::now();
    send(
        &state,
        SendArgs {
            pane: pane.to_owned(),
            text: "hello".to_owned(),
            enter: true,
        },
    )
    .await
    .expect("the supervisor holds this pane");
    let waiting = started.elapsed();

    assert_eq!(
        state.pty.last_output_at(pane),
        quiet,
        "the premise: this pane said nothing while the text was typed at it"
    );
    assert!(
        waiting >= ENTER_WAIT,
        "the Enter went without waiting for the pane: {waiting:?}"
    );
    state.pty.kill(pane).unwrap();
}

/// And the Enter still submits. The arithmetic is the point: a shell that ran
/// the line prints something the line itself does not contain, so the echo
/// cannot be mistaken for the answer.
#[tokio::test]
async fn the_enter_submits_what_was_typed() {
    let (_dir, state) = AppState::for_tests();
    let pane = "a-pane-that-runs-what-it-is-told";
    let (tx, rx) = channel();
    let sink: OutputSink = Box::new(move |out| {
        let _ = tx.send(out);
    });
    let program = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
    state
        .pty
        .spawn(
            SpawnRequest {
                pane_id: pane.to_owned(),
                program: PathBuf::from(program),
                args: Vec::new(),
                cwd: std::env::temp_dir(),
                env: vec![("DEX_PANE_ID".to_owned(), pane.to_owned())],
                cols: 80,
                rows: 24,
            },
            sink,
        )
        .unwrap();

    let text = if cfg!(windows) {
        "set /a 7*191".to_owned()
    } else {
        "expr 7 \\* 191".to_owned()
    };
    send(
        &state,
        SendArgs {
            pane: pane.to_owned(),
            text,
            enter: true,
        },
    )
    .await
    .expect("the supervisor holds this pane");

    assert!(
        heard(&state, pane, &rx, "1337", Duration::from_secs(10)),
        "the shell never ran the line, so the Enter never landed"
    );
    state.pty.kill(pane).unwrap();
}

/// Whether the pane says `needle`, playing the terminal while it waits:
/// ConPTY renders nothing until its opening cursor query is answered, as
/// xterm.js answers it in the app.
fn heard(
    state: &AppState,
    pane: &str,
    rx: &Receiver<PtyOutput>,
    needle: &str,
    within: Duration,
) -> bool {
    let deadline = Instant::now() + within;
    let mut seen = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(PtyOutput::Data(data)) => {
                if data.windows(4).any(|window| window == b"\x1b[6n") {
                    let _ = state.pty.write(pane, b"\x1b[1;1R");
                }
                let _ = state.pty.ack(pane, data.len());
                seen.extend(data);
                if String::from_utf8_lossy(&seen).contains(needle) {
                    return true;
                }
            }
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

/// The same wiring from the other side, where the pty answers for the child.
///
/// A Unix pty echoes what is typed into it from the line discipline, so the
/// evidence is there at once and the Enter is not held up. It is weaker
/// evidence than it looks - the echo says the pty took the bytes, not that the
/// child read them - but a program that submits a prompt is in raw mode with
/// the echo off, and then the only thing that can draw is the program itself.
#[cfg(unix)]
#[tokio::test]
async fn a_pty_that_echoes_for_the_child_gives_the_evidence_at_once() {
    let (_dir, state) = AppState::for_tests();
    let pane = "a-pane-whose-pty-does-the-talking";
    a_silent_pane(&state, pane, Box::new(|_| {}));
    let quiet = once_it_is_quiet(&state, pane).await;

    let started = Instant::now();
    send(
        &state,
        SendArgs {
            pane: pane.to_owned(),
            text: "hello".to_owned(),
            enter: true,
        },
    )
    .await
    .expect("the supervisor holds this pane");
    let waiting = started.elapsed();

    assert_ne!(
        state.pty.last_output_at(pane),
        quiet,
        "the premise: the pty echoed the text even though the child never read it"
    );
    assert!(
        waiting < ENTER_WAIT,
        "the Enter waited for evidence it already had: {waiting:?}"
    );
    state.pty.kill(pane).unwrap();
}
