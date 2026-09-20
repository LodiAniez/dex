//! Waking an agent to read messages waiting for it (issue #58).
//!
//! A message is stored whatever the recipient is doing. A running agent sees
//! it in its next delta; an idle one has finished its turn and would sit there
//! for ever, so it is woken: the nudge below is typed into its pane, which
//! starts a turn in which it reads its inbox.
//!
//! Two moments can wake it - the message arriving, and the agent going idle
//! afterwards - so the decision and its mark are taken together here, and an
//! agent is woken once per message, however many hooks race.

use rusqlite::Connection;

use dex_protocol::agent::AgentStatus;
use dex_protocol::pane::SendArgs;

use super::logic;
use super::store;
use crate::app::AppState;
use crate::features::workspace;

/// Typed into an idle agent's pane. Unlike a digest this is a prompt, so it
/// may say what to do; it says why it is there, because the agent sees only
/// the text, and keeps it short: the message itself is in the inbox.
pub const NUDGE: &str = "Another agent has sent you a message. Read it with message_inbox and act on it if it changes your task.";

/// Whether to wake this agent now, recorded as taken: the caller types
/// [`NUDGE`] into its pane. Recorded before the typing, so two hooks racing -
/// or a message arriving as the agent goes idle - wake it once.
pub fn take_wake(
    conn: &Connection,
    agent_id: &str,
    status: AgentStatus,
    started: bool,
) -> rusqlite::Result<bool> {
    let newest = store::newest_unread(conn, agent_id)?;
    if !logic::wake_for_messages(status, started, newest, store::woken_at(conn, agent_id)?) {
        return Ok(false);
    }
    // `newest` is Some: the rule said so.
    if let Some(seq) = newest {
        store::mark_woken(conn, agent_id, seq)?;
    }
    Ok(true)
}

/// Types the nudge into the agent's pane. Best effort, after the message is
/// safely stored: a pane whose shell is not running cannot be typed into, and
/// that is no reason to fail what the caller was doing - the agent still has
/// the message, and reads it at its next turn.
pub async fn nudge(state: &AppState, pane: String) {
    let typed = workspace::send(
        state,
        SendArgs {
            pane: pane.clone(),
            text: NUDGE.to_owned(),
            enter: true,
        },
    )
    .await;
    if let Err(err) = typed {
        tracing::debug!(%pane, %err, "could not wake the idle agent for its messages");
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
