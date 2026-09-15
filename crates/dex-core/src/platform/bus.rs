//! Broadcast bus telling listeners (the UI) that some state changed.
//!
//! Lossy by design: a slow listener skips notifications instead of stalling
//! the daemon, and every notification only means "re-read", so a skipped one
//! is covered by the next. Topics are plain strings, so this module stays
//! ignorant of what the features are.

use tokio::sync::broadcast;

pub use tokio::sync::broadcast::error::RecvError;

/// How far a listener can fall behind before it starts skipping.
const CAPACITY: usize = 64;

/// A change notification: which kind of state to re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Changed {
    /// What changed, e.g. `"workspaces"`.
    pub topic: &'static str,
}

/// The bus. Cheap to clone; clones share subscribers.
#[derive(Debug, Clone)]
pub struct Bus {
    tx: broadcast::Sender<Changed>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    /// A bus with no subscribers.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CAPACITY);
        Self { tx }
    }

    /// Announces a change. Having nobody listening is fine.
    pub fn publish(&self, topic: &'static str) {
        // Err only means "no subscribers right now", which is not a problem.
        let _ = self.tx.send(Changed { topic });
    }

    /// A new listener, receiving changes published from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Changed> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_hear_published_topics() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        bus.publish("workspaces");
        assert_eq!(rx.recv().await.unwrap().topic, "workspaces");
    }

    #[test]
    fn publishing_without_subscribers_is_fine() {
        Bus::new().publish("workspaces");
    }
}
