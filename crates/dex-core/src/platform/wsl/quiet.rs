//! Not asking a distro that has stopped answering (issue #73).
//!
//! Every probe of a distro has a 15 second limit, which is right for one slow
//! answer and wrong for a distro that has wedged: the watchdog sweeps every 15
//! seconds, so it would spend its whole life waiting on the same silence, and
//! everything the sweep does afterwards - ending agents whose Claude Code has
//! gone, waking the ones with messages waiting - waits with it.
//!
//! So a distro that does not answer is left alone for a while, and longer each
//! time: 30 seconds, then a minute, two, four, up to five. Asking again is what
//! ends the backoff; nothing else has to notice the distro came back.
//!
//! The memory is here rather than in `AppState` because it is the probes' own
//! business, and `platform` is where state that belongs to no feature lives.
//! Deciding nothing is lost by forgetting it: a fresh process asks once.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The first wait after a distro goes quiet.
const FIRST_MS: i64 = 30_000;

/// However long it has been quiet, it is asked again at least this often, so a
/// distro that comes back is noticed within five minutes without anyone
/// watching it.
const LONGEST_MS: i64 = 300_000;

/// When each distro that has gone quiet may be asked again, and how long the
/// wait after this one is.
struct Waiting {
    until: i64,
    next: i64,
}

fn waits() -> &'static Mutex<HashMap<String, Waiting>> {
    static WAITS: OnceLock<Mutex<HashMap<String, Waiting>>> = OnceLock::new();
    WAITS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether `distro` is inside a wait from an earlier silence, and so should not
/// be asked at all.
pub fn is_resting(distro: &str, now: i64) -> bool {
    let Ok(waits) = waits().lock() else {
        return false;
    };
    waits.get(distro).is_some_and(|waiting| now < waiting.until)
}

/// Records that a distro did not answer, and says how long it is now left for.
pub fn went_quiet(distro: &str, now: i64) -> i64 {
    let Ok(mut waits) = waits().lock() else {
        return FIRST_MS;
    };
    let waiting = waits.entry(distro.to_owned()).or_insert(Waiting {
        until: 0,
        next: FIRST_MS,
    });
    let wait = waiting.next;
    waiting.until = now.saturating_add(wait);
    waiting.next = (wait * 2).min(LONGEST_MS);
    wait
}

/// Records that a distro answered, so the next silence starts the wait again
/// from the beginning.
pub fn answered(distro: &str) {
    if let Ok(mut waits) = waits().lock() {
        waits.remove(distro);
    }
}

/// Every distro currently being left alone, for saying so in the window.
pub fn resting(now: i64) -> Vec<String> {
    let Ok(waits) = waits().lock() else {
        return Vec::new();
    };
    let mut quiet: Vec<String> = waits
        .iter()
        .filter(|(_, waiting)| now < waiting.until)
        .map(|(distro, _)| distro.clone())
        .collect();
    quiet.sort();
    quiet
}

#[cfg(test)]
mod tests {
    use super::{FIRST_MS, LONGEST_MS, answered, is_resting, resting, went_quiet};

    /// Tests share the one memory, so each uses a distro name of its own.
    #[test]
    fn a_distro_that_answers_is_always_asked() {
        assert!(!is_resting("answers", 1_000));
        answered("answers");
        assert!(!is_resting("answers", 1_000));
    }

    #[test]
    fn silence_is_left_alone_for_a_while_and_longer_each_time() {
        let first = went_quiet("quiet-one", 0);
        assert_eq!(first, FIRST_MS);
        assert!(is_resting("quiet-one", FIRST_MS - 1), "inside the wait");
        assert!(!is_resting("quiet-one", FIRST_MS), "and asked again after");

        let second = went_quiet("quiet-one", FIRST_MS);
        assert_eq!(second, FIRST_MS * 2, "each silence waits longer");
        let third = went_quiet("quiet-one", FIRST_MS * 3);
        assert_eq!(third, FIRST_MS * 4);
    }

    #[test]
    fn the_wait_never_grows_past_five_minutes() {
        for _ in 0..20 {
            went_quiet("stubborn", 0);
        }
        assert_eq!(
            went_quiet("stubborn", 0),
            LONGEST_MS,
            "a distro that comes back is noticed within five minutes"
        );
    }

    #[test]
    fn answering_clears_the_wait() {
        went_quiet("came-back", 0);
        assert!(is_resting("came-back", 1));

        answered("came-back");

        assert!(!is_resting("came-back", 1));
        assert_eq!(
            went_quiet("came-back", 0),
            FIRST_MS,
            "and the next silence starts from the beginning"
        );
    }

    #[test]
    fn what_is_resting_can_be_listed_for_the_window() {
        went_quiet("listed", 0);
        assert!(resting(1).contains(&"listed".to_owned()));
        assert!(
            !resting(FIRST_MS + 1).contains(&"listed".to_owned()),
            "once the wait is over it is not resting, it is simply next"
        );
        answered("listed");
    }
}
