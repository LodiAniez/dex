//! Where a pane's output goes. Normally straight to the window showing it; for
//! the moment a pane moves between windows - popped out into one of its own,
//! or docked back - output is held, then handed in order to the new window.
//!
//! The hand-off, which the app drives:
//! 1. `hold` - output from now on is kept here. It returns how many bytes the
//!    old window has been sent since it attached, so the old window can wait
//!    until it has written all of them before it serializes its screen.
//! 2. The old window serializes; the new one draws what it is given.
//! 3. `attach` - the new window's sink receives everything held, in order,
//!    and everything after.
//!
//! Nothing is lost and nothing is shown twice: every byte goes either to the
//! old window before `hold` or to the new one after `attach`.

use std::sync::Mutex;

use super::{OutputSink, PtyOutput};

enum Route {
    To(OutputSink),
    Held(Vec<PtyOutput>),
}

struct State {
    route: Route,
    /// Data bytes sent to the current sink since it was attached.
    delivered: u64,
}

/// A pane's output switchboard. Shared between its coalescer and the supervisor.
pub struct Relay {
    state: Mutex<State>,
}

impl Relay {
    pub fn new(sink: OutputSink) -> Self {
        Self {
            state: Mutex::new(State {
                route: Route::To(sink),
                delivered: 0,
            }),
        }
    }

    /// Passes one piece of output on, or keeps it while held.
    pub fn deliver(&self, output: PtyOutput) {
        let mut state = self.lock();
        match &mut state.route {
            Route::To(sink) => {
                let bytes = data_len(&output);
                sink(output);
                state.delivered += bytes;
            }
            Route::Held(kept) => kept.push(output),
        }
    }

    /// Keeps output here from now on. Returns the data bytes the current sink
    /// has been sent since it attached (0 if output was already held).
    pub fn hold(&self) -> u64 {
        let mut state = self.lock();
        match state.route {
            Route::Held(_) => 0,
            Route::To(_) => {
                state.route = Route::Held(Vec::new());
                std::mem::take(&mut state.delivered)
            }
        }
    }

    /// Sends `sink` everything held, in order, and everything after.
    pub fn attach(&self, mut sink: OutputSink) {
        let mut state = self.lock();
        let kept = match std::mem::replace(&mut state.route, Route::Held(Vec::new())) {
            Route::Held(kept) => kept,
            // Attached without a hold: nothing is owed to anyone.
            Route::To(_) => Vec::new(),
        };
        let mut delivered = 0;
        for output in kept {
            delivered += data_len(&output);
            sink(output);
        }
        state.route = Route::To(sink);
        state.delivered = delivered;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // A poisoned lock means a sink panicked; the route is still usable.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn data_len(output: &PtyOutput) -> u64 {
    match output {
        PtyOutput::Data(bytes) => bytes.len() as u64,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A sink that records what it receives.
    fn recorder() -> (OutputSink, Arc<Mutex<Vec<PtyOutput>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let into = seen.clone();
        let sink: OutputSink = Box::new(move |output| {
            if let Ok(mut all) = into.lock() {
                all.push(output);
            }
        });
        (sink, seen)
    }

    fn data(text: &str) -> PtyOutput {
        PtyOutput::Data(text.as_bytes().to_vec())
    }

    fn taken(seen: &Arc<Mutex<Vec<PtyOutput>>>) -> Vec<PtyOutput> {
        seen.lock().map(|all| all.clone()).unwrap_or_default()
    }

    #[test]
    fn output_goes_straight_to_the_window_showing_the_pane() {
        let (sink, seen) = recorder();
        let relay = Relay::new(sink);
        relay.deliver(data("hello"));
        assert_eq!(taken(&seen), vec![data("hello")]);
    }

    #[test]
    fn a_hand_off_loses_nothing_and_repeats_nothing() {
        let (old, old_seen) = recorder();
        let relay = Relay::new(old);
        relay.deliver(data("abc"));
        relay.deliver(data("de"));

        // The old window is told how much it was sent, to wait for all of it.
        assert_eq!(relay.hold(), 5);
        relay.deliver(data("while "));
        relay.deliver(data("moving"));

        let (new, new_seen) = recorder();
        relay.attach(new);
        relay.deliver(data("after"));

        assert_eq!(taken(&old_seen), vec![data("abc"), data("de")]);
        assert_eq!(
            taken(&new_seen),
            vec![data("while "), data("moving"), data("after")]
        );
    }

    #[test]
    fn a_process_that_exits_mid_hand_off_is_reported_to_the_new_window() {
        let (old, _) = recorder();
        let relay = Relay::new(old);
        relay.hold();
        relay.deliver(PtyOutput::Exited { code: Some(0) });
        let (new, new_seen) = recorder();
        relay.attach(new);
        assert_eq!(taken(&new_seen), vec![PtyOutput::Exited { code: Some(0) }]);
    }

    #[test]
    fn the_count_starts_again_with_each_window() {
        let (first, _) = recorder();
        let relay = Relay::new(first);
        relay.deliver(data("1234"));
        assert_eq!(relay.hold(), 4);
        relay.deliver(data("56"));
        let (second, _) = recorder();
        relay.attach(second);
        relay.deliver(data("789"));
        // The held "56" and the later "789" both went to the second window.
        assert_eq!(relay.hold(), 5);
    }

    #[test]
    fn holding_twice_owes_nothing_more() {
        let (sink, _) = recorder();
        let relay = Relay::new(sink);
        relay.deliver(data("abc"));
        assert_eq!(relay.hold(), 3);
        assert_eq!(relay.hold(), 0);
    }
}
