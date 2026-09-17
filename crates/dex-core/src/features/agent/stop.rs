//! `agent.stop`: ending an agent, and what becomes of its pane.

use std::time::Duration;

use dex_protocol::agent::{AgentStatus, StopAgentArgs, Stopped};
use dex_protocol::pane::{Key, SendArgs, SendKeyArgs};
use dex_protocol::workspace::PaneArgs;

use super::identity::find_target;
use super::logic::{self, ExitStep};
use super::model::AgentError;
use super::store;
use crate::app::AppState;
use crate::features::workspace::{self, WorkspaceError};
use crate::platform::clock;

/// Ctrl+C presses: the first interrupts a running turn, the next two quit.
const STOP_PRESSES: usize = 3;
const STOP_GAP: Duration = Duration::from_millis(300);
/// After Escape, before typing: Claude Code redraws its prompt.
const EXIT_SETTLE: Duration = Duration::from_millis(600);
/// How long Claude Code gets to leave when asked, before Ctrl+C.
const EXIT_WAIT: Duration = Duration::from_secs(6);
const EXIT_POLL: Duration = Duration::from_millis(250);

/// `agent.stop`: ends an agent by pressing Ctrl+C in its pane: the first press
/// interrupts a running turn, the next two quit Claude Code. A press left over
/// lands at the shell prompt, where it is harmless.
///
/// Claude Code killed this way does not get to run its `SessionEnd` hook
/// (verified against 2.1.273), so waiting for one would leave the agent showing
/// `running` until the watchdog noticed two minutes later. The row is ended here
/// instead; if Claude survived the interrupt, its next hook revives it
/// (`logic::revived`).
///
/// **The pane goes with a spawned agent.** Dex made that pane for it, so a lead
/// that ends its children, or an owner who clocks one out, is not left with a
/// row of empty shells. A pane the owner opened and ran `claude` in is theirs,
/// and stays unless they ask (`close_pane`). A workspace's last pane always stays.
///
/// **Gracefully** (`graceful`) asks first: `/exit`, after an Escape if the
/// agent is mid-turn, and a few seconds for its `SessionEnd` hook to report
/// it gone. Ctrl+C follows only if it is still there.
pub async fn stop(state: &AppState, args: StopAgentArgs) -> Result<Stopped, AgentError> {
    let target = args.agent.clone();
    let agent = state
        .db
        .call(move |conn| find_target(conn, &target))
        .await??;
    if agent.status == AgentStatus::Dead {
        return Err(AgentError::NotRunning(agent.id));
    }
    let spawned = agent.parent_id.is_some() || agent.depth > 0;

    if let Some(pane) = agent.pane_id.as_deref() {
        let left = args.graceful && ask_to_leave(state, &agent.id, pane, agent.status).await?;
        if !left {
            interrupt(state, pane).await?;
        }
    }
    // With no pane there is nothing to press Ctrl+C in — but the row still says
    // it is alive, and counted toward the spawn limit while `stop` refused to
    // touch it. Ending it is the whole of what stopping can mean there.
    let now = clock::now_millis();
    let id = agent.id.clone();
    state
        .db
        .call(move |conn| store::end_agent(conn, &id, now))
        .await?;

    let closed_pane = match &agent.pane_id {
        Some(pane) if spawned || args.close_pane => close(state, pane).await?,
        _ => false,
    };
    Ok(Stopped {
        agent: agent.id,
        pane: agent.pane_id,
        closed_pane,
    })
}

/// Asks Claude Code to leave, and says whether it did. A pane with no shell has
/// nobody to ask.
async fn ask_to_leave(
    state: &AppState,
    agent_id: &str,
    pane: &str,
    status: AgentStatus,
) -> Result<bool, AgentError> {
    let id = agent_id.to_owned();
    let started = state
        .db
        .call(move |conn| store::has_session(conn, &id))
        .await?;
    let plan = logic::exit_plan(status, started);
    if plan.is_empty() {
        return Ok(false);
    }
    for step in plan {
        let sent = match step {
            ExitStep::Escape => {
                let key = SendKeyArgs {
                    pane: pane.to_owned(),
                    key: Key::Escape,
                };
                let sent = workspace::send_key(state, key).await;
                tokio::time::sleep(EXIT_SETTLE).await;
                sent
            }
            ExitStep::Type(text) => {
                let line = SendArgs {
                    pane: pane.to_owned(),
                    text: text.to_owned(),
                    enter: true,
                };
                workspace::send(state, line).await
            }
        };
        match sent {
            Ok(_) => {}
            Err(WorkspaceError::PaneNotStarted(_)) => return Ok(false),
            Err(err) => return Err(err.into()),
        }
    }
    let deadline = tokio::time::Instant::now() + EXIT_WAIT;
    while tokio::time::Instant::now() < deadline {
        let id = agent_id.to_owned();
        let gone = state
            .db
            .call(move |conn| find_target(conn, &id))
            .await?
            .map(|agent| agent.status == AgentStatus::Dead)
            .unwrap_or(true);
        if gone {
            return Ok(true);
        }
        tokio::time::sleep(EXIT_POLL).await;
    }
    Ok(false)
}

/// Presses Ctrl+C until Claude Code has quit. A pane whose shell never started
/// has nothing running in it to interrupt, which is not a reason to refuse.
async fn interrupt(state: &AppState, pane: &str) -> Result<(), AgentError> {
    for press in 0..STOP_PRESSES {
        if press > 0 {
            tokio::time::sleep(STOP_GAP).await;
        }
        let key = SendKeyArgs {
            pane: pane.to_owned(),
            key: Key::CtrlC,
        };
        match workspace::send_key(state, key).await {
            Ok(_) => {}
            Err(WorkspaceError::PaneNotStarted(_)) => return Ok(()),
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

/// Closes the pane, and says whether it did. The UI ends the pane's shell when
/// it sees the pane gone, exactly as when the owner closes one.
async fn close(state: &AppState, pane: &str) -> Result<bool, AgentError> {
    let args = PaneArgs {
        pane: pane.to_owned(),
    };
    match workspace::close_pane(state, args).await {
        Ok(_) => {
            state.bus.publish("workspaces");
            Ok(true)
        }
        // A workspace keeps at least one pane; one already gone is as good as closed.
        Err(WorkspaceError::LastPane | WorkspaceError::NoSuchPane(_)) => Ok(false),
        Err(err) => Err(err.into()),
    }
}
