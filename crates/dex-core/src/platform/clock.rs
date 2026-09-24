//! Wall-clock time, in the unit every timestamp column uses.
//!
//! A plain function, and it stays one: the rules that depend on elapsed time -
//! the watchdog's `is_silent`, the quiet line's `quiet_for`, the digest rate
//! limit - are pure functions taking `now`, so their tests pass their own and
//! never need this one moved.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        // A clock set before 1970 is misconfigured; zero keeps ordering sane.
        .unwrap_or(0)
}
