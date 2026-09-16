//! Waiting for a pane to print an expected phrase, then typing an answer once.
//!
//! This exists for one job: a spawned agent's Claude Code stops at a trust
//! dialog in its brand-new worktree, and a spawned agent has no human to press
//! the key (docs/prd.md §9.4). Answering on a timer would be a guess at a
//! prompt that may not be there; this waits until the words actually appear.
//!
//! The supervisor never interprets output otherwise — the display owns that.
//! An answer is armed per pane, fires at most once, and expires.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Answers armed per pane id.
pub(super) type Watches = Arc<Mutex<HashMap<String, Answer>>>;

/// How much recent output to keep while waiting for the phrase.
const SEEN_CAP: usize = 8 * 1024;

/// A phrase to wait for, and the keys to type when it arrives.
pub struct Answer {
    /// The phrase, reduced the same way the output is.
    needle: String,
    /// Each key as its own write, in order. Claude Code's input handling reads
    /// keys that arrive together as a paste (ARCHITECTURE.md), so they are sent
    /// one at a time.
    keys: Vec<Vec<u8>>,
    /// How long to wait after the phrase appears before the first key. Claude
    /// Code ignores input for 150ms after a dialog opens, and every ignored key
    /// restarts that window, so typing early would be worse than typing late.
    settle: Duration,
    /// How long to wait between keys.
    gap: Duration,
    /// When to stop waiting.
    deadline: Instant,
    /// Output seen so far, reduced, most recent `SEEN_CAP` bytes.
    seen: String,
    /// Where the escape-sequence stripper is.
    esc: Esc,
}

impl Answer {
    /// Waits up to `within` for `phrase`, then types `keys`.
    pub fn new(phrase: &str, keys: Vec<Vec<u8>>, within: Duration) -> Self {
        let mut needle = String::new();
        reduce_str(phrase, &mut needle);
        Self {
            needle,
            keys,
            settle: Duration::from_millis(500),
            gap: Duration::from_millis(200),
            deadline: Instant::now() + within,
            seen: String::new(),
            esc: Esc::None,
        }
    }

    /// Adds output; true once the phrase has appeared.
    fn observe(&mut self, bytes: &[u8]) -> bool {
        for &byte in bytes {
            self.esc = self.esc.step(byte, &mut self.seen);
        }
        if self.seen.len() > SEEN_CAP {
            self.seen.drain(..self.seen.len() - SEEN_CAP);
        }
        !self.needle.is_empty() && self.seen.contains(&self.needle)
    }
}

/// Where the escape-sequence stripper is in a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Esc {
    None,
    /// Saw ESC; the next byte says what kind of sequence this is.
    Saw,
    /// Inside `ESC [ … final`.
    Csi,
    /// Inside a string sequence (OSC, DCS, APC, PM), which runs to BEL or ST.
    Str,
}

impl Esc {
    /// Consumes one byte, appending it to `out` only if it is part of the text.
    ///
    /// Reducing to letters and digits is what makes the phrase findable at all.
    /// A terminal may break it anywhere: the pane wraps lines, and Claude Code
    /// draws dialogs inside a box, so a continuation line starts with a border
    /// glyph. Escape sequences have to go separately, because their parameters
    /// are digits and their final bytes are letters — `ESC[1m` in the middle of
    /// a sentence would otherwise insert `1m` into the middle of the phrase.
    fn step(self, byte: u8, out: &mut String) -> Self {
        match self {
            Esc::None if byte == 0x1b => Esc::Saw,
            Esc::None => {
                if byte.is_ascii_alphanumeric() {
                    out.push(byte.to_ascii_lowercase() as char);
                }
                Esc::None
            }
            Esc::Saw => match byte {
                b'[' => Esc::Csi,
                b']' | b'P' | b'X' | b'^' | b'_' => Esc::Str,
                // Anything else is a two-byte sequence, now complete.
                _ => Esc::None,
            },
            // Parameters and intermediates are 0x20..=0x3f; a byte at 0x40 or
            // above ends the sequence.
            Esc::Csi if (0x40..=0x7e).contains(&byte) => Esc::None,
            Esc::Csi => Esc::Csi,
            // BEL ends a string sequence, and so does ST, whose first byte is ESC.
            Esc::Str if byte == 0x07 || byte == 0x1b => Esc::None,
            Esc::Str => Esc::Str,
        }
    }
}

fn reduce_str(text: &str, out: &mut String) {
    let mut esc = Esc::None;
    for byte in text.bytes() {
        esc = esc.step(byte, out);
    }
}

/// Feeds a pane's output to its armed answer, and types it if the phrase came.
pub(super) fn observe(watches: &Watches, panes: &super::Panes, pane_id: &str, bytes: &[u8]) {
    let ready = {
        let mut armed = lock(watches);
        let Some(answer) = armed.get_mut(pane_id) else {
            return;
        };
        if Instant::now() > answer.deadline {
            armed.remove(pane_id);
            tracing::debug!(pane = %pane_id, "expected prompt never appeared");
            return;
        }
        if !answer.observe(bytes) {
            return;
        }
        armed.remove(pane_id)
    };
    let Some(answer) = ready else { return };

    // On its own thread: the keys are spaced out, and this is the coalescer,
    // which has to keep draining the PTY meanwhile or the child would block.
    let panes = panes.clone();
    let pane = pane_id.to_owned();
    let started = super::spawn_named(format!("pty-answer-{pane}"), move || {
        thread::sleep(answer.settle);
        for key in answer.keys {
            if super::write_to(&panes, &pane, &key).is_err() {
                return;
            }
            thread::sleep(answer.gap);
        }
        tracing::info!(pane = %pane, "answered an expected prompt");
    });
    if let Err(err) = started {
        tracing::warn!(pane = %pane_id, %err, "could not answer an expected prompt");
    }
}

/// Arms an answer, replacing any the pane already had.
pub(super) fn arm(watches: &Watches, pane_id: &str, answer: Answer) {
    lock(watches).insert(pane_id.to_owned(), answer);
}

/// Disarms a pane's answer; called when its process exits.
pub(super) fn disarm(watches: &Watches, pane_id: &str) {
    lock(watches).remove(pane_id);
}

fn lock(watches: &Watches) -> std::sync::MutexGuard<'_, HashMap<String, Answer>> {
    watches
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(phrase: &str) -> Answer {
        Answer::new(phrase, vec![b"\x1b[B".to_vec()], Duration::from_secs(60))
    }

    #[test]
    fn the_phrase_is_found_in_plain_output() {
        let mut watch = answer("Yes, I trust this folder");
        assert!(!watch.observe(b"Quick safety check\r\n"));
        assert!(watch.observe(b"  Yes, I trust this folder\r\n"));
    }

    #[test]
    fn a_phrase_split_by_wrapping_is_still_found() {
        // The whole reason the output is reduced: the pane wraps mid-word and
        // the next line starts with the dialog's box border.
        let mut watch = answer("Yes, I trust this folder");
        assert!(watch.observe("  Yes, I trust this fol\r\n\u{2502} der\r\n".as_bytes()));
    }

    #[test]
    fn colour_changes_inside_the_phrase_do_not_hide_it() {
        // `ESC[1m` between two words must not leave `1m` in the middle of it.
        let mut watch = answer("Yes, I trust this folder");
        assert!(watch.observe(b"Yes, I trust \x1b[1mthis\x1b[0m folder"));
    }

    #[test]
    fn escape_sequences_alone_never_match() {
        let mut watch = answer("38");
        assert!(!watch.observe(b"\x1b[38;5;240mhello\x1b[0m"));
        assert!(watch.observe(b"38"));
    }

    #[test]
    fn a_string_sequence_is_skipped_whole() {
        // An OSC title carries arbitrary text; it is not pane output.
        let mut watch = answer("trust this folder");
        assert!(!watch.observe(b"\x1b]0;trust this folder\x07"));
    }

    #[test]
    fn the_phrase_is_found_across_two_chunks() {
        let mut watch = answer("Yes, I trust this folder");
        assert!(!watch.observe(b"Yes, I tru"));
        assert!(watch.observe(b"st this folder"));
    }

    #[test]
    fn old_output_is_forgotten_but_the_tail_is_kept() {
        let mut watch = answer("Yes, I trust this folder");
        assert!(!watch.observe(&vec![b'x'; SEEN_CAP * 2]));
        assert!(watch.seen.len() <= SEEN_CAP);
        assert!(watch.observe(b"Yes, I trust this folder"));
    }

    #[test]
    fn the_first_key_waits_out_claude_codes_refusal_window() {
        // Claude Code ignores input for 150ms after a dialog opens and restarts
        // that window on every ignored key, so early keys are not just wasted.
        let watch = answer("anything");
        assert!(watch.settle >= Duration::from_millis(300));
    }
}
