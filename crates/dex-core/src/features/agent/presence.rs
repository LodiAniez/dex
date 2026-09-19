//! An agent is Claude Code running in a pane; a pane where it no longer runs
//! has no agent in it.
//!
//! Claude Code does not always get to say goodbye: quit with Ctrl+C, crashed,
//! or killed, it fires no `SessionEnd`, and what its hooks last said - "idle" -
//! stands over a bare shell. The watchdog therefore looks at the process table
//! (`platform::proctree`) and ends whoever is not there. Only on evidence: a
//! pane with no process of its own, or a table that could not be read, ends
//! nobody. A pane inside WSL is not in Windows' table at all; for those the
//! distro is asked which of its processes run Claude Code, and for which pane
//! (`platform::wsl::claude_panes`), and a distro that does not answer ends
//! nobody either.
//!
//! Reading the table costs a PowerShell start, so it is read only when there
//! is a started agent to look for, and a second time only to confirm a
//! departure: Claude Code replaces its own process when it updates, and gone
//! for a moment is not gone.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use dex_protocol::agent::AgentStatus;

use super::model::Agent;
use super::{AgentError, identity, stop, store};
use crate::app::AppState;
use crate::features::{context, workspace};
use crate::platform::clock;
use crate::platform::proctree::{self, Presence, Proc};
use crate::platform::wsl::{self, Runtime};

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

/// Which of `candidates` - agent id and its pane - in a distro where `running`
/// are the panes that run Claude Code, have none.
pub fn gone_in_wsl(running: &[String], candidates: &[(String, String)]) -> HashSet<String> {
    candidates
        .iter()
        .filter(|(_, pane)| !running.contains(pane))
        .map(|(id, _)| id.clone())
        .collect()
}

/// Whoever was gone both times they were looked for, in a stable order.
pub fn confirmed(first: &HashSet<String>, second: &HashSet<String>) -> Vec<String> {
    let mut both: Vec<String> = first.intersection(second).cloned().collect();
    both.sort();
    both
}

/// A living, started agent whose pane has a live shell.
struct Candidate {
    agent: String,
    pane: String,
    shell: u32,
    /// Where the pane runs: `windows` or `wsl:<distro>`.
    runtime: String,
}

async fn candidates(state: &AppState) -> Result<Vec<Candidate>, AgentError> {
    let listed = state
        .db
        .call(|conn| {
            store::presence_candidates(conn)?
                .into_iter()
                .map(|(agent, pane)| {
                    let runtime = workspace::pane_runtime(conn, &pane)?.unwrap_or_default();
                    Ok((agent, pane, runtime))
                })
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;
    Ok(listed
        .into_iter()
        .filter_map(|(agent, pane, runtime)| {
            let shell = state.pty.shell_pid(&pane)?;
            Some(Candidate {
                agent,
                pane,
                shell,
                runtime,
            })
        })
        .collect())
}

async fn look(candidates: &[Candidate]) -> HashSet<String> {
    let mut departed = HashSet::new();
    let mut on_windows = Vec::new();
    let mut in_wsl: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for c in candidates {
        match Runtime::parse(&c.runtime) {
            Ok(Runtime::Wsl(distro)) => in_wsl
                .entry(distro)
                .or_default()
                .push((c.agent.clone(), c.pane.clone())),
            _ => on_windows.push((c.agent.clone(), c.shell)),
        }
    }
    if !on_windows.is_empty() {
        match tokio::task::spawn_blocking(proctree::snapshot).await {
            Ok(Ok(procs)) => departed.extend(gone(&procs, &on_windows)),
            Ok(Err(err)) => {
                tracing::warn!(%err, "could not read the process table; nobody is ended on a guess");
            }
            Err(_) => {}
        }
    }
    for (distro, panes) in in_wsl {
        let asked = distro.clone();
        match tokio::task::spawn_blocking(move || wsl::claude_panes(&asked)).await {
            Ok(Ok(running)) => departed.extend(gone_in_wsl(&running, &panes)),
            Ok(Err(err)) => {
                tracing::warn!(%err, %distro, "could not ask the distro; nobody in it is ended on a guess");
            }
            Err(_) => {}
        }
    }
    departed
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
