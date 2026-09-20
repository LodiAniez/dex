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
/// and says in which panes it set out to. Never fatal: the messages keep, and
/// a nudge that could not be typed gives its wake back for the next sweep.
pub async fn wake_waiting(state: &AppState) -> Result<Vec<String>, AgentError> {
    let claimed = state
        .db
        .call(move |conn| -> rusqlite::Result<Vec<context::Wake>> {
            let mut claimed = Vec::new();
            for agent in store::list_agents(conn, false)? {
                let Some(pane) = agent.pane_id.clone() else {
                    continue;
                };
                if !waits_at_its_prompt(agent.status)
                    || asking::asked_the_owner(agent.status_detail.as_deref())
                {
                    continue;
                }
                let started = store::has_session(conn, &agent.id)?;
                let wake = context::take_wake(conn, &agent.id, pane, started, agent.status_at)?;
                claimed.extend(wake);
            }
            Ok(claimed)
        })
        .await?;
    let mut panes = Vec::new();
    for wake in claimed {
        let pane = wake.pane().to_owned();
        context::wake_up(state, wake).await;
        panes.push(pane);
    }
    Ok(panes)
}

/// Whether an agent in this status is sitting at its prompt with nothing to do.
///
/// `unknown` is here because it is what a lost `Stop` hook looks like: the
/// agent finished its turn two minutes ago and Dex never heard, so nothing
/// else will ever wake it. Typing at one that turns out to be working costs
/// nothing - Claude Code queues the text as the next prompt - whereas
/// `waiting` (a dialog is open, and typed text answers it) and `error` are
/// left alone, and the dead are never listed.
fn waits_at_its_prompt(status: AgentStatus) -> bool {
    matches!(status, AgentStatus::Idle | AgentStatus::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_agent_at_its_prompt_is_woken() {
        assert!(waits_at_its_prompt(AgentStatus::Idle));
        // Gone quiet: a Stop hook Dex never received looks exactly like this.
        assert!(waits_at_its_prompt(AgentStatus::Unknown));
        assert!(!waits_at_its_prompt(AgentStatus::Running));
        // A dialog is open: the nudge would be typed as its answer.
        assert!(!waits_at_its_prompt(AgentStatus::Waiting));
        assert!(!waits_at_its_prompt(AgentStatus::Error));
        assert!(!waits_at_its_prompt(AgentStatus::Dead));
    }
}
