//! `agent.sweep`: the watchdog, run on a timer by the app. What hooks cannot
//! say for themselves - a pane that is gone, a Claude Code that is gone, an
//! agent gone quiet - is noticed here.

use dex_protocol::agent::{AgentStatus, EventOutcome};

use super::model::AgentError;
use super::{presence, silence, store};
use crate::app::AppState;
use crate::platform::clock;

/// A running agent with no hook event and no output for this long is `unknown`.
const WATCHDOG_MS: i64 = 120_000;

const APPLIED: EventOutcome = EventOutcome { applied: true };
const IGNORED: EventOutcome = EventOutcome { applied: false };

/// `agent.sweep`: the watchdog. Agents whose pane is gone are ended, and so are
/// those whose pane no longer runs Claude Code (`presence`); running
/// agents with no hook event and no pane output for two minutes become
/// `unknown`, which covers every way hook delivery can fail. Announces a
/// change only when it made one.
pub async fn sweep(state: &AppState) -> Result<EventOutcome, AgentError> {
    let statuses = sweep_statuses(state).await?;
    // Before anyone is typed at: an agent whose Claude Code has gone still
    // looks idle, and its pane is a bare shell that would run the nudge as a
    // command. It may take a couple of seconds and announces what it does
    // itself; never fatal - what the rest of the sweep found still stands.
    let departed = presence::end_the_departed(state)
        .await
        .unwrap_or_else(|err| {
            tracing::warn!(%err, "could not look for agents whose Claude Code has gone");
            0
        });
    // Agents that finished a turn with messages waiting: woken here, a beat
    // after the turn, rather than in the hooks Claude Code waits on.
    let woken = super::waking::wake_waiting(state)
        .await
        .unwrap_or_else(|err| {
            tracing::warn!(%err, "could not wake the agents with messages waiting");
            Vec::new()
        })
        .len();
    Ok(if departed > 0 || woken > 0 {
        APPLIED
    } else {
        statuses
    })
}

async fn sweep_statuses(state: &AppState) -> Result<EventOutcome, AgentError> {
    let now = clock::now_millis();
    // Orphans first: a pane closed from the UI, or lost with a crashed
    // session, nulls `pane_id` but leaves status where it was. Left alone,
    // those rows counted toward the spawn limit for ever.
    let orphaned = state
        .db
        .call(move |conn| store::end_orphans(conn, now))
        .await?;
    let running = state
        .db
        .call(|conn| store::list_by_status(conn, AgentStatus::Running))
        .await?;
    let silent: Vec<String> = running
        .into_iter()
        .filter(|agent| {
            let output = agent
                .pane_id
                .as_deref()
                .and_then(|pane| state.pty.last_output_at(pane));
            silence::is_silent(agent.status, agent.last_event_at, output, now, WATCHDOG_MS)
        })
        .map(|agent| agent.id)
        .collect();
    if silent.is_empty() {
        if orphaned > 0 {
            state.bus.publish("agents");
            return Ok(APPLIED);
        }
        return Ok(IGNORED);
    }
    state
        .db
        .call(move |conn| {
            for id in &silent {
                store::update_status(conn, id, AgentStatus::Unknown, None, now, None)?;
            }
            Ok(())
        })
        .await?;
    state.bus.publish("agents");
    Ok(APPLIED)
}
