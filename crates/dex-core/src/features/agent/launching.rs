//! Starting the child's Claude Code once its pane has a shell, and ending the
//! agent when that never happens.
//!
//! Split out of `spawn.rs`, which owns the six-step sequence up to the point
//! it returns; this is what then happens on a task of its own.

use std::time::Duration;

use crate::app::AppState;
use crate::features::context;
use crate::platform::clock;
use crate::platform::pty::Answer;

use super::{logic, store};

/// Exactly what is typed into the child's shell. A constant: no user text ever
/// reaches a command line.
const KICKOFF: &str = "Begin the task described in your workspace context.";

/// How long to wait for the child's shell to exist before giving up on the
/// launch. The window starts a pane's shell as soon as it hears the pane
/// exists, wherever that pane is (issue #92); this is the margin on that.
const LAUNCH_WAIT: Duration = Duration::from_secs(30);
const LAUNCH_POLL: Duration = Duration::from_millis(250);

/// The confirm option of Claude Code's folder-trust dialog, which it shows the
/// first time it runs anywhere — including in a worktree Dex has just made.
const TRUST_PROMPT: &str = "Yes, I trust this folder";
/// Down, then Enter: the dialog lists "No, exit" first and focuses it, and
/// hides the index numbers, so there is no key that picks an option directly.
const TRUST_KEYS: [&[u8]; 2] = [b"\x1b[B", b"\r"];
/// How long to keep watching for the dialog. Generous: Claude Code has to
/// start, and a cold start on a large repository is not quick.
const TRUST_WAIT: Duration = Duration::from_secs(120);

/// Why an agent that was never launched ended, in its status and in the log.
const NEVER_STARTED: &str = "never started";

/// Types the kickoff prompt once the pane's shell exists.
///
/// The window starts the shell when it hears about the pane, which has not
/// happened when `spawn` returns, so this waits in the background rather than
/// failing.
///
/// Text and Enter go as separate writes: Claude Code's input box reads a
/// carriage return arriving with the text as a pasted newline (ARCHITECTURE.md).
///
/// **The trust dialog.** Claude Code asks whether it may work in a directory
/// the first time it runs there — and for the home directory, every time — so
/// a spawned agent would sit at that dialog forever with nobody to answer it,
/// which defeats the milestone (PRD §14 M7). Dex answers it for every child,
/// because a child never lands anywhere the owner did not already choose: the
/// parent's own folder (where the owner started Claude Code themselves), a
/// repository they registered, or a worktree Dex made from one. This was first
/// limited to worktrees; the first real use spawned into the parent's folder
/// and three children stalled at the dialog. The settings-trust dialog is never
/// answered: whether to run the hooks a project's config declares is a
/// different question, and the owner's.
pub(super) fn launch(launch: Launch) {
    tokio::spawn(async move {
        let Launch {
            state,
            pane,
            agent_id,
            mode,
            chrome,
            answer_trust,
        } = launch;
        let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;
        while tokio::time::Instant::now() < deadline {
            if state.pty.last_output_at(&pane).is_some() {
                if answer_trust {
                    // Armed before the command, so the watch is in place however
                    // fast Claude Code reaches the dialog.
                    state.pty.answer_once(
                        &pane,
                        Answer::new(
                            TRUST_PROMPT,
                            TRUST_KEYS.map(<[u8]>::to_vec).into(),
                            TRUST_WAIT,
                        ),
                    );
                }
                let command = logic::launch_command(&mode, chrome, KICKOFF);
                let pty = state.pty.clone();
                let pane = pane.clone();
                let _ =
                    tokio::task::spawn_blocking(move || pty.write(&pane, command.as_bytes())).await;
                return;
            }
            tokio::time::sleep(LAUNCH_POLL).await;
        }
        // No shell ever appeared, so no Claude Code was launched: the row must
        // not sit `idle` counting toward the spawn limit as if it might still.
        tracing::warn!(
            agent = %agent_id,
            "the spawned agent's pane never started a shell; ending it unlaunched"
        );
        never_started(&state, &agent_id).await;
    });
}

/// Ends an agent that was never launched, and says so in the workspace log.
///
/// The log line is the point: `dex agent list` leaves a dead agent out unless
/// it is asked for, and the lead that spawned this one is not watching the
/// daemon's own log - so without this the brief simply vanished (issue #92).
/// Said exactly as every other ending is, so the activity pane and the office
/// read it without knowing about this case.
pub(super) async fn never_started(state: &AppState, agent_id: &str) {
    let now = clock::now_millis();
    let id = agent_id.to_owned();
    let ended = state
        .db
        .call(move |conn| -> rusqlite::Result<bool> {
            let Some(agent) = store::find_agent(conn, &id)? else {
                return Ok(false);
            };
            store::update_status(
                conn,
                &id,
                dex_protocol::agent::AgentStatus::Dead,
                Some(NEVER_STARTED),
                now,
                Some(now),
            )?;
            let label = super::identity::label_of(conn, &id)?.unwrap_or_else(|| "an agent".into());
            let body = format!("{label} is dead ({NEVER_STARTED})");
            context::record_status(conn, &agent.workspace_id, &id, body, now)?;
            Ok(true)
        })
        .await;
    if ended.unwrap_or(false) {
        state.bus.publish("agents");
    }
}

/// What `launch` needs once the pane's shell appears.
pub(super) struct Launch {
    pub(super) state: AppState,
    pub(super) pane: String,
    pub(super) agent_id: String,
    /// The permission mode, taken from the same snapshot as the agent row's, so
    /// the pane header and the command line can never disagree.
    pub(super) mode: String,
    /// Whether the child may connect to Claude in Chrome (`agents.spawn_chrome`).
    pub(super) chrome: bool,
    /// Whether to answer Claude Code's folder-trust dialog for this child.
    /// Always, today; kept as a field so the reason is a decision in `spawn`
    /// and not an accident of `launch`.
    pub(super) answer_trust: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kickoff_line_never_carries_the_brief() {
        // The whole point of the fixed constant: no user text reaches a shell,
        // so there is no quoting or length problem to get wrong (PRD §9.4).
        assert!(!KICKOFF.contains('$'));
        assert!(!KICKOFF.contains('`'));
        assert!(!KICKOFF.contains('\n'));
        assert_eq!(
            KICKOFF,
            "Begin the task described in your workspace context."
        );
    }

    #[test]
    fn the_trust_dialog_is_answered_with_down_then_enter() {
        // It lists "No, exit" first and focuses it, and hides the index
        // numbers, so Enter alone would quit and no digit selects anything.
        assert_eq!(TRUST_KEYS[0], b"\x1b[B");
        assert_eq!(TRUST_KEYS[1], b"\r");
    }
}
