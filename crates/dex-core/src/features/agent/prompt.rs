//! `agent.prompt`: typing the owner's turn into an agent's terminal, and above
//! all when not to.
//!
//! The office's Prompt box and its loudspeaker used to type straight into the
//! agent's pane. A pane is only Claude Code while Claude Code is running in
//! it: for the first seconds after a hire, and after a crash, it is a shell,
//! and a shell runs what is typed into it. The wake-up nudge learned this in
//! 0.1.1; this is the same rule, in the one place that can know.

use dex_protocol::agent::{AgentStatus, PromptAgentArgs, Prompted};
use dex_protocol::pane::SendArgs;

use super::identity::find_target;
use super::model::AgentError;
use super::store;
use crate::app::AppState;
use crate::features::workspace;
use crate::platform::clock;
use crate::platform::proctree::{self, Presence};

/// Why an agent cannot be typed at right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptRefusal {
    /// Its pane is closed.
    NoPane,
    /// Claude Code has not started in its pane yet: the pane is a bare shell.
    NotStarted,
    /// It is at a permission dialog, which typed text would answer.
    WaitingForOwner,
    /// Stopped with an error, or gone quiet: Claude Code may not be there.
    OutOfReach,
    /// Its hooks say it is fine, but no Claude Code process runs in its pane:
    /// it was quit with Ctrl+C or crashed, and never got to say so.
    ClaudeGone,
}

impl PromptRefusal {
    /// A few words for the owner.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoPane => "it has no pane",
            Self::NotStarted => "Claude Code has not started in its pane yet",
            Self::WaitingForOwner => "it is waiting on a dialog, which typed text would answer",
            Self::OutOfReach => "Claude Code may not be running in its pane",
            Self::ClaudeGone => "Claude Code is no longer running in its pane",
        }
    }
}

/// Whether text may be typed at an agent in this state, and if not, why.
pub fn prompt_refusal(status: AgentStatus, started: bool, has_pane: bool) -> Option<PromptRefusal> {
    if !has_pane {
        return Some(PromptRefusal::NoPane);
    }
    if !started {
        return Some(PromptRefusal::NotStarted);
    }
    match status {
        AgentStatus::Idle | AgentStatus::Running => None,
        AgentStatus::Waiting => Some(PromptRefusal::WaitingForOwner),
        AgentStatus::Error | AgentStatus::Unknown | AgentStatus::Dead => {
            Some(PromptRefusal::OutOfReach)
        }
    }
}

/// A prompt as one line: in a terminal a newline is Enter, and half a prompt
/// submitted early is worse than one with its line breaks flattened.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `agent.prompt`: types `text` into the agent's terminal and presses Enter,
/// if and only if Claude Code is there to read it.
pub async fn prompt(state: &AppState, args: PromptAgentArgs) -> Result<Prompted, AgentError> {
    let text = one_line(&args.text);
    if text.is_empty() {
        return Err(AgentError::EmptyPrompt);
    }
    let target = args.agent;
    let (agent, started) = state
        .db
        .call(move |conn| -> rusqlite::Result<Result<_, AgentError>> {
            let agent = match find_target(conn, &target)? {
                Ok(agent) => agent,
                Err(err) => return Ok(Err(err)),
            };
            let started = store::has_session(conn, &agent.id)?;
            Ok(Ok((agent, started)))
        })
        .await??;
    if agent.status == AgentStatus::Dead {
        return Err(AgentError::NotRunning(agent.id));
    }
    if let Some(refusal) = prompt_refusal(agent.status, started, agent.pane_id.is_some()) {
        return Err(AgentError::NotPromptable(refusal));
    }
    let pane = agent.pane_id.unwrap_or_default();
    if claude_has_gone(state, &pane).await {
        // What its hooks last said is no longer true. Say so for everyone:
        // the office stops showing someone who is not there.
        let now = clock::now_millis();
        let id = agent.id.clone();
        state
            .db
            .call(move |conn| store::end_agent(conn, &id, now))
            .await?;
        state.bus.publish("agents");
        return Err(AgentError::NotPromptable(PromptRefusal::ClaudeGone));
    }
    let line = SendArgs {
        pane: pane.clone(),
        text,
        enter: true,
    };
    workspace::send(state, line).await?;
    Ok(Prompted {
        agent: agent.id,
        pane,
    })
}

/// Whether the process table shows that no Claude Code runs in the pane.
///
/// An agent's status is what its hooks last said, and Claude Code quit with
/// Ctrl+C, or crashed, fires none: the agent goes on reading "idle" over a bare
/// shell. Only evidence refuses a prompt - a pane with no process of its own, a
/// table that could not be read, or a WSL pane the table cannot see into all
/// let it through to the rule above.
async fn claude_has_gone(state: &AppState, pane: &str) -> bool {
    let Some(shell) = state.pty.shell_pid(pane) else {
        return false;
    };
    match tokio::task::spawn_blocking(proctree::snapshot).await {
        Ok(Ok(procs)) => proctree::claude_under(&procs, shell) == Presence::Gone,
        Ok(Err(err)) => {
            tracing::warn!(%err, "could not read the process table; prompting on the agent's status alone");
            false
        }
        Err(_) => false,
    }
}
