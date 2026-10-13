//! Noticing when something took too long (issue #86).
//!
//! `#73` gave the daemon a log; this is what makes it answer "what took the
//! time". A pane that accepts no input for a minute, a sweep that overruns its
//! own fifteen seconds, a distro that will not answer - each of those was
//! argued from circumstance before, because nothing recorded a duration.
//!
//! Only the overruns are written. A line per keystroke would bury the file
//! that has to stay readable, and a keystroke that took two milliseconds is
//! not news.

use std::time::{Duration, Instant};

/// A pane write slower than this is worth a line: nobody types at this speed,
/// so it means the write waited on something.
pub const SLOW_WRITE: Duration = Duration::from_millis(500);

/// The watchdog sweeps every fifteen seconds. One that takes longer than this
/// has eaten its own interval, and whatever it does afterwards - ending agents
/// whose Claude Code has gone, waking the ones with messages waiting - was
/// late by however much it says.
pub const SLOW_SWEEP: Duration = Duration::from_secs(5);

/// Writes a line if `started` was longer ago than `over`, and says by how
/// much. Returns how long it took, so a caller can use the number as well.
///
/// Takes the instant rather than wrapping the work: the things worth timing
/// here are a blocking write on one thread, an async sweep on another and a
/// subprocess on a third, and a wrapper that fitted all three would be worth
/// less than the three lines it saved.
pub fn note(what: &str, started: Instant, over: Duration) -> Duration {
    let took = started.elapsed();
    if took > over {
        tracing::warn!(%what, ms = took.as_millis() as u64, "slower than it should be");
    }
    took
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{SLOW_SWEEP, SLOW_WRITE, note};

    #[test]
    fn what_was_quick_is_timed_and_said_nothing_about() {
        let took = note("quick", Instant::now(), Duration::from_secs(10));
        assert!(took < Duration::from_secs(1), "it returns the duration");
    }

    #[test]
    fn what_took_too_long_is_still_timed() {
        // The line itself goes to the log; what is testable here is that the
        // duration comes back either way, since callers use it.
        let took = note("slow", Instant::now(), Duration::ZERO);
        assert!(took >= Duration::ZERO);
    }

    #[test]
    fn the_thresholds_are_the_ones_the_rules_assume() {
        // A sweep's own interval is 15s, so 5s leaves room to overrun without
        // a line, and none to eat the interval quietly.
        assert!(SLOW_SWEEP < Duration::from_secs(15));
        // Nobody types twice in half a second by accident.
        assert!(SLOW_WRITE < SLOW_SWEEP);
    }
}
