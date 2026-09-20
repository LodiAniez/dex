//! Waking agents that sit idle with messages waiting (issue #58).
//!
//! A message reaches a working agent in its next delta, but one that has
//! finished its turn has no next turn until someone gives it one. The watchdog
//! looks for those a beat after they finish - not the hooks themselves, which
//! Claude Code waits on, and where typed text would land in a session still
//! starting up - and types the nudge into their panes.
//!
//! An agent that ended its turn by asking the owner something is left alone:
//! it is idle, but the pane is waiting for the owner's answer, and the nudge
//! would be typed as that answer.

use dex_protocol::agent::AgentStatus;

use super::{AgentError, asking, store};
use crate::app::AppState;
use crate::features::context;

/// Wakes every agent sitting idle with messages it has not been woken for,
/// and says in which panes. Never fatal: the messages keep.
pub async fn wake_waiting(state: &AppState) -> Result<Vec<String>, AgentError> {
    let panes = state
        .db
        .call(move |conn| -> rusqlite::Result<Vec<String>> {
            let mut waking = Vec::new();
            for agent in store::list_agents(conn, false)? {
                let Some(pane) = agent.pane_id.clone() else {
                    continue;
                };
                if agent.status != AgentStatus::Idle
                    || asking::asked_the_owner(agent.status_detail.as_deref())
                {
                    continue;
                }
                let started = store::has_session(conn, &agent.id)?;
                if context::take_wake(conn, &agent.id, started, agent.status_at)? {
                    waking.push(pane);
                }
            }
            Ok(waking)
        })
        .await?;
    for pane in &panes {
        context::nudge(state, pane.clone()).await;
    }
    Ok(panes)
}
