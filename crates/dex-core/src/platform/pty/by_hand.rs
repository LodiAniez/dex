//! Telling the owner's typing apart from the terminal's own answers, and
//! marking a pane while they are at it (issue #103).
//!
//! The window sends everything xterm.js emits down one path (`pty_write`, fed
//! by `onData`), and that is not only what a person types. The terminal also
//! answers questions on its own: the cursor-position report that answers
//! ConPTY's opening `ESC[6n` - which `pty.rs` documents and every pane's shell
//! start produces - the device-attributes replies, and focus in and out as the
//! window is alt-tabbed. Marking a pane on those would mark every pane as the
//! owner's the moment it opened, and a program that asks where the cursor is
//! would keep it marked for as long as it kept asking: a nudge that defers for
//! ever is an agent that is never woken, which is the feature this would have
//! broken (issue #58).
//!
//! So the mark is set for everything except those answers. Keys cannot be
//! mistaken for one: arrows are `ESC [ A` to `ESC [ D`, function keys
//! `ESC [ n ~` or `ESC O P`, and none of them end in `R` or `c`. A mouse
//! report still counts as the owner: clicking in a pane is being at it.

use super::writing;
use super::{Panes, PtyError};
use crate::platform::clock;

/// Sends input from the window, marking the pane as the owner's unless the
/// bytes are the terminal answering a question.
///
/// The mark is set before the write and not after: a write to a pane that is
/// not reading blocks for as long as it likes, and the owner was typing either
/// way.
pub(super) fn write(panes: &Panes, pane_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
    if !is_terminal_reply(bytes) {
        mark_typed(panes, pane_id);
    }
    writing::write_to(panes, pane_id, bytes)
}

/// Records that someone typed into this pane just now. A pane that has gone is
/// not an error here: the write that follows says so.
fn mark_typed(panes: &Panes, pane_id: &str) {
    let mut panes = panes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(pane) = panes.get_mut(pane_id) {
        pane.typed = clock::now_millis();
    }
}

/// When a person last typed into this pane, unix millis (0 if never); `None`
/// if the pane has no live process.
pub(super) fn last_typed(panes: &Panes, pane_id: &str) -> Option<i64> {
    let panes = panes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    panes.get(pane_id).map(|pane| pane.typed)
}

/// Whether these bytes are the terminal answering questions rather than
/// somebody typing: cursor-position reports, device-attributes replies, and
/// focus in or out.
///
/// The whole chunk must be answers, and it may be several of them. The window
/// batches what it has to send while a write is in flight
/// (`terminalRegistry.sendInput`), so two answers can arrive as one chunk -
/// and a chunk with a keystroke anywhere in it is a chunk the owner typed in.
pub(super) fn is_terminal_reply(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    if rest.is_empty() {
        return false;
    }
    while let Some(after) = one_reply(rest) {
        rest = after;
        if rest.is_empty() {
            return true;
        }
    }
    false
}

/// One answer at the front of `bytes`, and whatever follows it; `None` if what
/// is at the front is not an answer.
fn one_reply(bytes: &[u8]) -> Option<&[u8]> {
    let body = bytes.strip_prefix(b"\x1b[")?;
    // Focus in and out, as the window is alt-tabbed.
    if let Some(rest) = body.strip_prefix(b"I").or_else(|| body.strip_prefix(b"O")) {
        return Some(rest);
    }
    // `ESC [ ... R` reports the cursor and `ESC [ ... c` the terminal's
    // identity; between the two only digits and the punctuation they use. A
    // key sequence ends in something else - arrows in `A` to `D`, the editing
    // keys in `~` - so none of them gets this far.
    let end = body
        .iter()
        .position(|byte| *byte == b'R' || *byte == b'c')?;
    let (head, rest) = body.split_at(end);
    let parameters = head
        .iter()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b';' | b'?' | b'>'));
    (!head.is_empty() && parameters).then(|| &rest[1..])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::tests::{cmd, collecting_sink};
    use super::super::{FlowLimits, PtySupervisor, SpawnRequest};
    use super::*;

    /// A child that sits and reads its input, so the pane stays live.
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

    #[test]
    fn the_answer_to_conptys_opening_question_is_not_typing() {
        // What xterm.js sends back for `ESC[6n`, in both the shapes it uses.
        assert!(is_terminal_reply(b"\x1b[1;1R"));
        assert!(is_terminal_reply(b"\x1b[?24;80R"));
    }

    #[test]
    fn a_terminals_identity_is_not_typing() {
        assert!(is_terminal_reply(b"\x1b[?6c"));
        assert!(is_terminal_reply(b"\x1b[?1;2c"));
        assert!(is_terminal_reply(b"\x1b[>85;95;0c"));
    }

    #[test]
    fn focus_in_and_out_are_not_typing() {
        assert!(is_terminal_reply(b"\x1b[I"));
        assert!(is_terminal_reply(b"\x1b[O"));
    }

    #[test]
    fn keys_are_typing() {
        for key in [
            &b"a"[..],
            b"hello",
            b"\x1b[A",    // up
            b"\x1b[D",    // left
            b"\x1b[1;5C", // ctrl+right
            b"\x1b[3~",   // delete
            b"\x1b[11~",  // F1 on some keymaps
            b"\x1bOP",    // F1 on others: SS3, not CSI
            b"\x1b[Z",    // shift+tab
            b"\x1b",      // escape itself
            b"c",         // the letter the identity reply ends with
            b"R",         // and the letter the cursor report ends with
            b"\r",
            b"\x1b[<0;8;1M", // a click: the owner is at the pane
        ] {
            assert!(
                !is_terminal_reply(key),
                "{:?} is somebody typing",
                String::from_utf8_lossy(key)
            );
        }
    }

    #[test]
    fn a_reply_batched_with_a_keystroke_is_typing() {
        // The window batches; a chunk with a key in it is the owner's.
        assert!(!is_terminal_reply(b"\x1b[1;1Rhello"));
        assert!(!is_terminal_reply(b"hello\x1b[1;1R"));
    }

    /// The case the first version of this got wrong: a pane marks itself as
    /// the owner's as its shell starts, because ConPTY asks where the cursor
    /// is and xterm.js answers down the same path the owner types on. Every
    /// pane would have opened "being typed in".
    #[test]
    fn the_answer_to_the_opening_question_leaves_a_fresh_pane_unmarked() {
        let supervisor = PtySupervisor::new(FlowLimits::default());
        let (sink, _rx) = collecting_sink();
        supervisor.spawn(interactive("answering"), sink).unwrap();

        // Exactly what xterm.js sends back for ConPTY's `ESC[6n`.
        supervisor.write_by_hand("answering", b"\x1b[1;1R").unwrap();
        assert_eq!(
            supervisor.last_typed_at("answering"),
            Some(0),
            "the terminal answering a question is not the owner typing"
        );

        supervisor
            .write_by_hand("answering", b"now they type")
            .unwrap();
        assert!(
            supervisor
                .last_typed_at("answering")
                .is_some_and(|at| at > 0),
            "and a keystroke on the same path still marks it"
        );
        supervisor.kill("answering").unwrap();
    }

    /// The mark says the owner typed here, so only their path may set it. Dex's
    /// own writes must leave it alone, or the first nudge would mark the pane
    /// and every later one would defer behind it.
    #[test]
    fn only_typing_by_hand_marks_a_pane_as_the_owners() {
        let supervisor = PtySupervisor::new(FlowLimits::default());
        let (sink, _rx) = collecting_sink();
        supervisor.spawn(interactive("marked"), sink).unwrap();

        assert_eq!(
            supervisor.last_typed_at("marked"),
            Some(0),
            "a pane nobody has typed into"
        );

        supervisor.write("marked", b"dex typed this\r").unwrap();
        assert_eq!(
            supervisor.last_typed_at("marked"),
            Some(0),
            "Dex's own write is not the owner typing"
        );

        supervisor
            .write_by_hand("marked", b"the owner typed this")
            .unwrap();
        let marked = supervisor.last_typed_at("marked").expect("a live pane");
        assert!(marked > 0, "typing by hand marks the pane: {marked}");

        supervisor.write("marked", b"\r").unwrap();
        assert_eq!(
            supervisor.last_typed_at("marked"),
            Some(marked),
            "and a later write by Dex does not move the mark"
        );
        supervisor.kill("marked").unwrap();
    }

    #[test]
    fn a_pane_that_has_gone_has_no_mark_and_is_not_an_error() {
        let supervisor = PtySupervisor::new(FlowLimits::default());
        assert_eq!(supervisor.last_typed_at("never-lived"), None);
        // The write is what reports it, as it always did.
        assert!(matches!(
            supervisor.write_by_hand("never-lived", b"x"),
            Err(PtyError::NoSuchPane(_))
        ));
    }

    #[test]
    fn several_answers_in_one_chunk_are_still_not_typing() {
        // The window batches while a write is in flight, so a shell start can
        // send the cursor report and an identity reply as one chunk.
        assert!(is_terminal_reply(b"\x1b[1;1R\x1b[?6c"));
        assert!(is_terminal_reply(b"\x1b[I\x1b[1;1R"));
    }
}
