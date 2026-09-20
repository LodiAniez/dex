//! Waking an agent to read messages waiting for it (issue #58).
//!
//! A message is stored whatever the recipient is doing. A running agent sees
//! it in its next delta; one that has finished its turn would sit there for
//! ever, so it is woken: the nudge below is typed into its pane, which starts
//! a turn in which it reads its inbox.
//!
//! Two moments can wake it - the message arriving, and the agent going idle
//! afterwards - so the decision and its mark are taken together here, and an
//! agent is woken once per message however many hooks race. A nudge that never
//! lands gives the mark back, so the next sweep tries again.

use std::time::Duration;

use rusqlite::Connection;

use dex_protocol::pane::SendArgs;

use super::logic;
use super::store;
use crate::app::AppState;
use crate::features::workspace;

/// Typed into an idle agent's pane. Unlike a digest this is a prompt, so it
/// may say what to do; it says why it is there, because the agent sees only
/// the text, and keeps it short: the message itself is in the inbox.
pub const NUDGE: &str = "Another agent has sent you a message. Read it with message_inbox and act on it if it changes your task.";

/// How long the typing may take before it is given up on. A shell that is not
/// reading its input blocks the write (`workspace/pane_io.rs`), and the
/// watchdog waits on every nudge before it can sweep again.
const TYPING: Duration = Duration::from_secs(2);

/// A wake claimed for one agent: which pane to type into, and what its mark
/// said before, so a nudge that never lands can be given back.
pub struct Wake {
    agent_id: String,
    pane: String,
    /// `(woken_at, woken_idle_at)` as they stood before the claim.
    previous: (i64, i64),
    /// What the claim wrote, so a wake taken since is not given back over.
    claimed: (i64, i64),
}

impl Wake {
    /// The pane the nudge goes into.
    pub fn pane(&self) -> &str {
        &self.pane
    }
}

/// Claims a wake for this idle agent, or `None` if it needs none. `idle_at` is
/// when it last changed status; the caller types [`NUDGE`] with [`wake_up`].
///
/// Recorded before the typing, and in one step with the decision, so a message
/// arriving as the watchdog looks wakes the agent once.
pub fn take_wake(
    conn: &Connection,
    agent_id: &str,
    pane: String,
    started: bool,
    idle_at: i64,
) -> rusqlite::Result<Option<Wake>> {
    let newest = store::newest_unread(conn, agent_id)?;
    let previous = store::woken(conn, agent_id)?;
    if !logic::wake_for_messages(started, newest, previous.0, previous.1, idle_at) {
        return Ok(None);
    }
    // `newest` is Some: the rule said so.
    let Some(seq) = newest else {
        return Ok(None);
    };
    store::mark_woken(conn, agent_id, seq, idle_at)?;
    Ok(Some(Wake {
        agent_id: agent_id.to_owned(),
        pane,
        previous,
        claimed: (seq, idle_at),
    }))
}

/// What became of the typing.
enum Typed {
    /// In the pane: the agent has been woken.
    Landed,
    /// Nothing was written - the pane has no shell to write to.
    Refused,
    /// The write is still out there, in a pane that is not reading it.
    TimedOut,
}

/// Types the nudge into the agent's pane, and says whether it landed. A pane
/// whose shell is not running cannot be typed into, and that is no reason to
/// fail what the caller was doing: the wake is given back instead, so the next
/// sweep tries again, and the sender is told the message waits for a turn.
///
/// A write that timed out keeps the wake. Bytes may yet arrive, and a pane
/// that blocks its writes would otherwise collect a fresh nudge every sweep,
/// each one holding the PTY lock, for as long as it stays stuck.
pub async fn wake_up(state: &AppState, wake: Wake) -> bool {
    match nudge(state, &wake.pane).await {
        Typed::Landed => true,
        Typed::Refused => {
            give_back(state, wake).await;
            false
        }
        Typed::TimedOut => false,
    }
}

async fn nudge(state: &AppState, pane: &str) -> Typed {
    let typing = workspace::send(
        state,
        SendArgs {
            pane: pane.to_owned(),
            text: NUDGE.to_owned(),
            enter: true,
        },
    );
    match tokio::time::timeout(TYPING, typing).await {
        Ok(Ok(_)) => Typed::Landed,
        Ok(Err(err)) => {
            tracing::debug!(%pane, %err, "could not wake the idle agent for its messages");
            Typed::Refused
        }
        Err(_) => {
            tracing::warn!(%pane, "gave up typing the nudge: the shell is not reading it");
            Typed::TimedOut
        }
    }
}

/// Puts the mark back as it was, so the agent is claimed again next sweep -
/// unless it has been woken for something newer since this wake was claimed,
/// which is the one case where putting it back would lose a wake.
///
/// Best effort, like the nudge itself: a mark that could not be given back
/// leaves that message waiting for a newer one, and is worth a warning rather
/// than failing the send or the sweep.
async fn give_back(state: &AppState, wake: Wake) {
    let given = state
        .db
        .call(move |conn| store::set_woken(conn, &wake.agent_id, wake.previous, wake.claimed))
        .await;
    match given {
        Ok(false) => tracing::debug!("a newer wake was taken while the nudge was being typed"),
        Ok(true) => {}
        Err(err) => tracing::warn!(%err, "could not give back a wake whose nudge never landed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nudge_tells_the_agent_how_to_read_the_message() {
        assert!(NUDGE.contains("message_inbox"));
        assert!(!NUDGE.contains('\n'), "one line: it is typed as a prompt");
    }
}
