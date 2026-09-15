//! Wall-clock time, in the unit every timestamp column uses.
//!
//! A plain function for now. It becomes an injectable source when the first
//! logic that depends on elapsed time lands (the agent watchdog, delta rate
//! limiting); until then tests have nothing to substitute.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        // A clock set before 1970 is misconfigured; zero keeps ordering sane.
        .unwrap_or(0)
}
