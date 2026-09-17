//! An agent is Claude Code running in a pane; a pane where it no longer runs
//! has no agent in it.
//!
//! Claude Code does not always get to say goodbye: quit with Ctrl+C, crashed,
//! or killed, it fires no `SessionEnd`, and what its hooks last said - "idle" -
//! stands over a bare shell. The watchdog therefore looks at the process table
//! (`platform::proctree`) and ends whoever is not there. Only on evidence: a
//! pane with no process of its own, a table that could not be read, or a pane
//! inside WSL, which the table cannot see into, ends nobody.
//!
//! Reading the table costs a PowerShell start, so it is read only when there
//! is a started agent to look for, and a second time only to confirm a
//! departure: Claude Code replaces its own process when it updates, and gone
//! for a moment is not gone.

use std::collections::HashSet;
use std::time::Duration;

use dex_protocol::agent::AgentStatus;

use super::model::Agent;
use super::{AgentError, identity, stop, store};
use crate::app::AppState;
use crate::features::context;
use crate::platform::clock;
use crate::platform::proctree::{self, Presence, Proc};

/// How long after finding someone gone the table is read again.
const LOOK_AGAIN_AFTER: Duration = Duration::from_secs(2);

/// Which of `candidates` - agent id and its pane's shell pid - the table shows
/// to have no Claude Code under their shell.
pub fn gone(procs: &[Proc], candidates: &[(String, u32)]) -> HashSet<String> {
    candidates
        .iter()
        .filter(|(_, shell)| proctree::claude_under(procs, *shell) == Presence::Gone)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Whoever was gone both times they were looked for, in a stable order.
pub fn confirmed(first: &HashSet<String>, second: &HashSet<String>) -> Vec<String> {
    let mut both: Vec<String> = first.intersection(second).cloned().collect();
    both.sort();
    both
}

/// Living, started agents with the pid of their pane's shell.
async fn candidates(state: &AppState) -> Result<Vec<(String, u32)>, AgentError> {
    let listed = state
        .db
        .call(|conn| store::presence_candidates(conn))
        .await?;
    Ok(listed
        .into_iter()
        .filter_map(|(id, pane)| state.pty.shell_pid(&pane).map(|shell| (id, shell)))
        .collect())
}

async fn look(candidates: &[(String, u32)]) -> HashSet<String> {
    match tokio::task::spawn_blocking(proctree::snapshot).await {
        Ok(Ok(procs)) => gone(&procs, candidates),
        Ok(Err(err)) => {
            tracing::warn!(%err, "could not read the process table; nobody is ended on a guess");
            HashSet::new()
        }
        Err(_) => HashSet::new(),
    }
}

/// Ends every agent whose pane no longer runs Claude Code. Returns how many.
pub async fn end_the_departed(state: &AppState) -> Result<usize, AgentError> {
    let looked_for = candidates(state).await?;
    if looked_for.is_empty() {
        return Ok(0);
    }
    let first = look(&looked_for).await;
    if first.is_empty() {
        return Ok(0);
    }
    tokio::time::sleep(LOOK_AGAIN_AFTER).await;
    // Looked for afresh: a pane may have closed, or a new session started in it, meanwhile.
    let still = candidates(state).await?;
    let departed = confirmed(&first, &look(&still).await);
    end_confirmed(state, departed).await
}

/// Why, in the workspace log: a lead reads it, and a child that crashed must
/// not simply stop answering.
const WHY: &str = "Claude Code is no longer running in its pane";

/// Ends `departed`, says so in the workspace log as any other end is said, and
/// closes the panes Dex made for them - a pane the owner opened stays, a shell
/// that may say why. Whoever ended in the meantime is left alone. Returns how
/// many were ended.
pub async fn end_confirmed(state: &AppState, departed: Vec<String>) -> Result<usize, AgentError> {
    if departed.is_empty() {
        return Ok(0);
    }
    let now = clock::now_millis();
    let ended: Vec<Agent> = state
        .db
        .call(move |conn| {
            let mut ended = Vec::new();
            for id in &departed {
                let Some(agent) = store::find_agent(conn, id)? else {
                    continue;
                };
                if agent.status == AgentStatus::Dead {
                    continue;
                }
                store::end_agent(conn, id, now)?;
                let label = identity::label_of(conn, id)?.unwrap_or_else(|| "an agent".into());
                let body = format!("{label} is dead ({WHY})");
                context::record_status(conn, &agent.workspace_id, id, body, now)?;
                ended.push(agent);
            }
            Ok(ended)
        })
        .await?;
    if ended.is_empty() {
        return Ok(0);
    }
    state.bus.publish("agents");
    for agent in &ended {
        tracing::info!(agent = %agent.id, "{WHY}; ended");
        let spawned = agent.parent_id.is_some() || agent.depth > 0;
        if let (true, Some(pane)) = (spawned, agent.pane_id.as_deref()) {
            // Ended either way: a pane that would not close is not worth failing the sweep over.
            if let Err(err) = stop::close(state, pane).await {
                tracing::warn!(%err, agent = %agent.id, "could not close the pane Dex made for it");
            }
        }
    }
    Ok(ended.len())
}
