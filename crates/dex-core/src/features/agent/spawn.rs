//! `agent.spawn`: one agent starting another (docs/prd.md §9.4).
//!
//! **The guardrails are the point.** A skill that can spawn agents can spawn
//! them recursively, and the failure mode is a fork bomb of processes each
//! costing real tokens. Depth and concurrency are enforced here, in the daemon,
//! not in the skill's prose, which a model may ignore.
//!
//! **The brief never goes through a shell.** The launch command is a fixed
//! constant; the brief reaches the child through its opening `SessionStart`
//! digest, stated as a fact. That removes every quoting and length problem, and
//! keeps the digest free of instructions. The kickoff prompt arrives as a real
//! user turn, which is what makes the child start working without a human.

use std::time::Duration;

use dex_protocol::agent::{SpawnArgs, Spawned};
use dex_protocol::workspace::{SplitDirection, SplitPaneArgs};

use super::model::{Agent, AgentError};
use super::store;
use crate::app::AppState;
use crate::features::{context, repo, workspace};
use crate::platform::pty::Answer;
use crate::platform::{clock, ids};

/// How deep spawning may go: a child may spawn, its child may not (PRD §13).
const MAX_DEPTH: i64 = 2;
/// How many agents may be alive in one workspace at once.
const MAX_CONCURRENT: i64 = 6;
/// The permission mode spawned agents run in, so they do not stop for approval.
const SPAWN_PERMISSION_MODE: &str = "auto";

/// Exactly what is typed into the child's shell. A constant: no user text ever
/// reaches a command line.
const KICKOFF: &str = "Begin the task described in your workspace context.";

/// How long to wait for the child's shell to exist before giving up on the
/// launch. The pane's shell starts when the UI first shows it.
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

/// `agent.spawn`: a new pane, a new agent, and a Claude Code already working.
pub async fn spawn(state: &AppState, args: SpawnArgs) -> Result<Spawned, AgentError> {
    let brief = args.task.trim().to_owned();
    if brief.is_empty() {
        return Err(AgentError::EmptyBrief);
    }
    let caller = caller(state, &args).await?;
    check_limits(state, &caller).await?;

    // Step 2: the worktree, before anything else is created. A spawn that fell
    // back to the main checkout would put two agents in one working tree.
    let (repo_id, cwd, branch) = match (&args.repo, &args.worktree) {
        (Some(repo), Some(branch)) => {
            let (repo_id, path) = repo::create_for_spawn(state, repo, branch).await?;
            (
                Some(repo_id),
                crate::platform::paths::normalize(&path),
                Some(branch.clone()),
            )
        }
        (Some(repo), None) => (None, repo_path(state, repo).await?, None),
        (None, Some(_)) => return Err(AgentError::WorktreeWithoutRepo),
        (None, None) => (None, caller.cwd.clone(), None),
    };
    // Whether Dex made this checkout itself, which is what licenses answering
    // the trust dialog in it — see `launch`.
    let own_worktree = repo_id.is_some();

    // Step 3: the pane, split from the caller's, inheriting its runtime.
    let direction = match args.direction.as_deref() {
        Some("down") => SplitDirection::Down,
        _ => SplitDirection::Right,
    };
    let list = workspace::split_pane(
        state,
        SplitPaneArgs {
            pane: caller.pane.clone(),
            direction,
            cwd: Some(cwd.clone()),
            label: args.label.clone(),
            kind: None,
        },
    )
    .await?;
    // The new pane takes focus in its workspace, which is how it is identified.
    let pane = list
        .workspaces
        .iter()
        .find(|ws| ws.id == caller.workspace_id)
        .and_then(|ws| ws.active_pane.clone())
        .ok_or_else(|| AgentError::NoSuchPane(caller.pane.clone()))?;

    // Steps 4 and 5: the agent row, then the spawn event.
    let agent_id = ids::new_id();
    let now = clock::now_millis();
    let row = Agent {
        id: agent_id.clone(),
        pane_id: Some(pane.clone()),
        workspace_id: caller.workspace_id.clone(),
        parent_id: caller.agent_id.clone(),
        depth: caller.depth + 1,
        label: args.label.clone(),
        backend: "claude".into(),
        status: dex_protocol::agent::AgentStatus::Idle,
        status_detail: None,
        status_at: now,
        permission_mode: Some(SPAWN_PERMISSION_MODE.into()),
        task_brief: Some(brief.clone()),
        started_at: now,
        last_event_at: now,
        ended_at: None,
    };
    let workspace_id = caller.workspace_id.clone();
    let parent_label = caller.label.clone();
    let event_brief = brief.clone();
    let spawned_id = agent_id.clone();
    state
        .db
        .call(move |conn| -> rusqlite::Result<()> {
            store::insert_agent(conn, &row, None)?;
            if let Some(repo_id) = repo_id.as_deref() {
                repo::attach_worktree(conn, &workspace_id, repo_id, &cwd, branch.as_deref())?;
            }
            context::record_event(
                conn,
                &workspace_id,
                Some(&spawned_id),
                "spawn",
                format!("{parent_label} started an agent for: {event_brief}"),
                now,
            )
        })
        .await?;

    // Step 6: launch, once the UI has started the pane's shell.
    launch(state.clone(), pane.clone(), agent_id.clone(), own_worktree);
    // A spawn changes two things, and the router only announces one. Without
    // this the new pane never reaches the UI, so no terminal mounts, so no
    // shell starts, so the child is never launched at all.
    state.bus.publish("workspaces");
    Ok(Spawned {
        agent: agent_id,
        pane,
        cwd: caller.cwd,
        branch: args.worktree,
    })
}

/// Who is spawning, and where.
struct Caller {
    pane: String,
    cwd: String,
    workspace_id: String,
    agent_id: Option<String>,
    label: String,
    depth: i64,
}

async fn caller(state: &AppState, args: &SpawnArgs) -> Result<Caller, AgentError> {
    let pane_target = args.pane.clone();
    let workspace_target = args.workspace.clone();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Caller, AgentError>> {
                let pane = match pane_target {
                    Some(pane) => pane,
                    None => match workspace_target.as_deref() {
                        // Without a pane, spawn from whatever the workspace has
                        // focused: a human at the CLI has no pane of their own.
                        Some(target) => match workspace::focused_pane(conn, target)? {
                            Ok(pane) => pane,
                            Err(err) => return Ok(Err(err.into())),
                        },
                        None => return Ok(Err(AgentError::NoCaller)),
                    },
                };
                let Some(workspace_id) = workspace::find_pane_workspace(conn, &pane)? else {
                    return Ok(Err(AgentError::NoSuchPane(pane)));
                };
                let parent = store::find_live_in_pane(conn, &pane)?;
                let cwd = workspace::pane_cwd(conn, &pane)?.unwrap_or_default();
                Ok(Ok(Caller {
                    label: match &parent {
                        Some(agent) => super::identity::label_of(conn, &agent.id)?
                            .unwrap_or_else(|| "an agent".into()),
                        None => "you".into(),
                    },
                    depth: parent.as_ref().map(|agent| agent.depth).unwrap_or(0),
                    agent_id: parent.map(|agent| agent.id),
                    pane,
                    workspace_id,
                    cwd,
                }))
            },
        )
        .await?
}

/// Refuses a spawn that would go too deep or crowd the workspace (PRD §9.4).
async fn check_limits(state: &AppState, caller: &Caller) -> Result<(), AgentError> {
    if caller.depth + 1 > MAX_DEPTH {
        return Err(AgentError::DepthLimit {
            depth: caller.depth + 1,
            max: MAX_DEPTH,
        });
    }
    let workspace_id = caller.workspace_id.clone();
    let live = state
        .db
        .call(move |conn| store::count_live_in_workspace(conn, &workspace_id))
        .await?;
    if live >= MAX_CONCURRENT {
        return Err(AgentError::ConcurrencyLimit {
            live,
            max: MAX_CONCURRENT,
        });
    }
    Ok(())
}

/// The path of a registered repo's main checkout.
async fn repo_path(state: &AppState, target: &str) -> Result<String, AgentError> {
    let list = repo::list(state).await?;
    list.repos
        .into_iter()
        .find(|repo| repo.name == target || repo.id == target)
        .map(|repo| repo.path)
        .ok_or_else(|| AgentError::NoSuchRepo(target.to_owned()))
}

/// Types the kickoff prompt once the pane's shell exists.
///
/// The shell starts when the UI first shows the pane, which has not happened
/// when `spawn` returns, so this waits in the background rather than failing.
/// Text and Enter go as separate writes: Claude Code's input box reads a
/// carriage return arriving with the text as a pasted newline (ARCHITECTURE.md).
///
/// **The trust dialog.** Claude Code asks whether it may work in a directory
/// the first time it runs there, and a worktree Dex has just created is always
/// new, so a spawned agent would sit at that dialog forever with nobody to
/// answer it — which defeats the milestone (PRD §14 M7). Dex answers it, but
/// only when `own_worktree`: the checkout is one Dex made, under Dex's own
/// worktree directory, from a repository the user registered themselves. It is
/// never answered for a directory the user pointed an agent at, and never for
/// the settings-trust dialog, which is a different question — whether to run
/// the hooks a project's config declares — and is the user's to answer.
fn launch(state: AppState, pane: String, agent_id: String, own_worktree: bool) {
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;
        while tokio::time::Instant::now() < deadline {
            if state.pty.last_output_at(&pane).is_some() {
                if own_worktree {
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
                let command =
                    format!("claude --permission-mode {SPAWN_PERMISSION_MODE} \"{KICKOFF}\"\r");
                let pty = state.pty.clone();
                let pane = pane.clone();
                let _ =
                    tokio::task::spawn_blocking(move || pty.write(&pane, command.as_bytes())).await;
                return;
            }
            tokio::time::sleep(LAUNCH_POLL).await;
        }
        eprintln!(
            "dex: agent {agent_id} was created but its pane never started a shell, so it was not launched"
        );
    });
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
    fn spawned_agents_do_not_stop_for_approval() {
        // `bypassPermissions` is only honoured in a worktree (PRD §9.4), and
        // Dex does not ship it as the default.
        assert_eq!(SPAWN_PERMISSION_MODE, "auto");
    }

    #[test]
    fn the_trust_dialog_is_answered_with_down_then_enter() {
        // It lists "No, exit" first and focuses it, and hides the index
        // numbers, so Enter alone would quit and no digit selects anything.
        assert_eq!(TRUST_KEYS[0], b"\x1b[B");
        assert_eq!(TRUST_KEYS[1], b"\r");
    }

    #[test]
    fn the_limits_are_the_documented_ones() {
        assert_eq!(MAX_DEPTH, 2);
        assert_eq!(MAX_CONCURRENT, 6);
    }
}
