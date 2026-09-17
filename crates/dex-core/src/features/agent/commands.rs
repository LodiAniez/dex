//! One handler per `agent.*` command: hook events, the list, pane exits, and
//! the watchdog sweep (docs/prd.md §9.1–9.2).

use dex_protocol::agent::{
    AgentEventArgs, AgentList, AgentStatus, AgentView, EventOutcome, ListAgentsArgs, PaneExitedArgs,
};
use rusqlite::Connection;

use super::identity;
use super::logic::{self, HookInput, HookKind, SessionStart};
use super::model::{Agent, AgentError};
use super::store;
use crate::app::AppState;
use crate::features::{context, workspace};
use crate::platform::{clock, ids};

/// A running agent with no hook event and no output for this long is `unknown`.
const WATCHDOG_MS: i64 = 120_000;

const APPLIED: EventOutcome = EventOutcome { applied: true };
const IGNORED: EventOutcome = EventOutcome { applied: false };

type Outcome = rusqlite::Result<Result<EventOutcome, AgentError>>;

/// `agent.event`: one Claude Code hook firing in a pane. Subagent hooks and
/// unknown kinds are ignored; everything else moves the pane's agent along
/// the lifecycle (PRD §9.2).
pub async fn event(state: &AppState, args: AgentEventArgs) -> Result<EventOutcome, AgentError> {
    let Some(kind) = HookKind::parse(&args.kind) else {
        return Ok(IGNORED);
    };
    let input = logic::read_input(&args.input);
    if input.from_subagent {
        return Ok(IGNORED);
    }
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| -> Outcome {
            let Some(workspace_id) = workspace::find_pane_workspace(conn, &args.pane)? else {
                return Ok(Err(AgentError::NoSuchPane(args.pane)));
            };
            let hook = Hook {
                args: &args,
                input: &input,
                kind,
                workspace_id,
                now,
            };
            if kind == HookKind::SessionStart {
                session_start(conn, &hook)?;
            } else {
                status_event(conn, &hook)?;
            }
            Ok(Ok(APPLIED))
        })
        .await?
}

/// One hook, with everything the handlers below need.
struct Hook<'a> {
    args: &'a AgentEventArgs,
    input: &'a HookInput,
    kind: HookKind,
    workspace_id: String,
    now: i64,
}

impl Hook<'_> {
    fn session(&self) -> Option<&str> {
        self.input.session_id.as_deref()
    }

    fn mode(&self) -> Option<&str> {
        self.input.permission_mode.as_deref()
    }

    /// Whether this hook shows `agent` outlived the moment Dex ended it.
    fn revives(&self, agent: &Agent) -> bool {
        logic::revived(agent.status, agent.ended_at, self.args.stamp)
    }
}

fn session_start(conn: &Connection, hook: &Hook<'_>) -> rusqlite::Result<()> {
    let pane = &hook.args.pane;
    if logic::session_start(hook.input.source.as_deref()) == SessionStart::Continues {
        // /clear or compaction: the same agent carries on, maybe under a new
        // session id, and its status does not change.
        if let Some(agent) = store::find_live_in_pane(conn, pane)? {
            return store::update_session(conn, &agent.id, hook.session(), hook.mode(), hook.now);
        }
    }
    let existing = match hook.session() {
        Some(session) => store::find_by_session(conn, pane, session)?,
        None => None,
    };
    // An agent Dex spawned was registered before its Claude Code started, and
    // carries the parent and brief the child needs. It is found by
    // `DEX_AGENT_ID`, or — if that never reached the shell — by being the one
    // row in this pane still waiting for a session to bind to. Missing it would
    // orphan the brief and start the child with no idea what it is for.
    let existing = match (existing, &hook.args.agent) {
        (None, Some(id)) => store::find_agent(conn, id)?,
        (None, None) => store::find_unbound_in_pane(conn, pane)?,
        (found, _) => found,
    };
    let existing = revive(conn, hook, existing)?;
    let agent = match existing {
        Some(agent) => {
            store::update_session(conn, &agent.id, hook.session(), hook.mode(), hook.now)?;
            agent
        }
        None => register(conn, hook, AgentStatus::Idle)?,
    };
    // A pane runs one Claude Code at a time: an earlier one that never sent
    // SessionEnd (killed, crashed) is over.
    store::end_live_in_pane(conn, pane, Some(&agent.id), hook.now)?;
    apply(conn, &agent, hook)
}

fn status_event(conn: &Connection, hook: &Hook<'_>) -> rusqlite::Result<()> {
    let pane = &hook.args.pane;
    let by_session = match hook.session() {
        Some(session) => store::find_by_session(conn, pane, session)?,
        None => None,
    };
    let by_session = revive(conn, hook, by_session)?;
    let agent = match by_session {
        Some(agent) => agent,
        // The session id changed without a SessionStart we saw: rebind the
        // pane's live agent rather than inventing a second one.
        None => match store::find_live_in_pane(conn, pane)? {
            Some(agent) => {
                store::update_session(conn, &agent.id, hook.session(), hook.mode(), hook.now)?;
                agent
            }
            // A session Dex never saw start (Dex started mid-session): register it now.
            None if hook.kind != HookKind::SessionEnd => {
                register(conn, hook, logic::status_after(hook.kind))?;
                return Ok(());
            }
            None => return Ok(()),
        },
    };
    store::touch(conn, &agent.id, hook.mode(), hook.now)?;
    apply(conn, &agent, hook)
}

/// Undoes an end Dex recorded too early. `agent.stop` ends a row without
/// waiting for a `SessionEnd` that a killed Claude Code never sends, and
/// `--resume` reuses a session id; a hook stamped after that end means the
/// session is alive. It keeps its row, because an agent *is* its
/// `(pane_id, session_id)` pair (PRD §9.1) — a second row would collide.
fn revive(
    conn: &Connection,
    hook: &Hook<'_>,
    agent: Option<Agent>,
) -> rusqlite::Result<Option<Agent>> {
    let Some(mut agent) = agent else {
        return Ok(None);
    };
    if hook.revives(&agent) {
        store::revive(conn, &agent.id, hook.args.stamp)?;
        agent.status = AgentStatus::Unknown;
        agent.status_at = hook.args.stamp;
        agent.ended_at = None;
    }
    Ok(Some(agent))
}

/// Applies the hook's transition to `agent`, if it is newer and changes anything.
fn apply(conn: &Connection, agent: &Agent, hook: &Hook<'_>) -> rusqlite::Result<()> {
    let Some(next) = logic::next_status(agent.status, agent.status_at, hook.kind, hook.args.stamp)
    else {
        // Still waiting, but on a new dialog: the reason follows the dialog on
        // screen. Nothing is logged - the agent's state has not changed.
        if logic::is_new_wait(agent.status, agent.status_at, hook.kind, hook.args.stamp) {
            let reason = super::reason::waiting_reason(&hook.args.input);
            store::update_status(
                conn,
                &agent.id,
                AgentStatus::Waiting,
                reason.as_deref(),
                hook.args.stamp,
                None,
            )?;
        }
        return Ok(());
    };
    // Why, where the hook says: what failed, or what the agent is waiting for.
    let detail = match next {
        AgentStatus::Error => Some(
            hook.input
                .failure
                .clone()
                .unwrap_or_else(|| "unknown".into()),
        ),
        AgentStatus::Waiting => super::reason::waiting_reason(&hook.args.input),
        _ => None,
    };
    let ended = (next == AgentStatus::Dead).then_some(hook.now);
    store::update_status(
        conn,
        &agent.id,
        next,
        detail.as_deref(),
        hook.args.stamp,
        ended,
    )?;
    record_status(conn, agent, hook, next, detail.as_deref())
}

/// Puts a status change in the workspace log, for the activity pane (PRD §10.1).
fn record_status(
    conn: &Connection,
    agent: &Agent,
    hook: &Hook<'_>,
    next: AgentStatus,
    detail: Option<&str>,
) -> rusqlite::Result<()> {
    let label = identity::label_of(conn, &agent.id)?.unwrap_or_else(|| "an agent".into());
    let because = detail.map(|why| format!(" ({why})")).unwrap_or_default();
    let body = format!("{label} is {}{because}", logic::status_name(next));
    context::record_status(conn, &hook.workspace_id, &agent.id, body, hook.now)
}

fn register(conn: &Connection, hook: &Hook<'_>, status: AgentStatus) -> rusqlite::Result<Agent> {
    let agent = Agent {
        id: ids::new_id(),
        pane_id: Some(hook.args.pane.clone()),
        workspace_id: hook.workspace_id.clone(),
        parent_id: None,
        depth: 0,
        label: None,
        backend: "claude".into(),
        status,
        status_detail: None,
        status_at: hook.args.stamp,
        permission_mode: hook.input.permission_mode.clone(),
        task_brief: None,
        started_at: hook.now,
        last_event_at: hook.now,
        ended_at: None,
    };
    store::insert_agent(conn, &agent, hook.session())?;
    Ok(agent)
}

/// `agent.list`: agents newest first, optionally including ended ones and
/// limited to one workspace (by id or name).
pub async fn list(state: &AppState, args: ListAgentsArgs) -> Result<AgentList, AgentError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<AgentList, AgentError>> {
                let scope = match (args.workspace.as_deref(), args.pane.as_deref()) {
                    (Some(target), _) => match workspace::workspace_id(conn, target)? {
                        Ok(id) => Some(id),
                        Err(err) => return Ok(Err(err.into())),
                    },
                    // A caller in a pane sees its own workspace's agents only.
                    (None, Some(pane)) => match workspace::find_pane_workspace(conn, pane)? {
                        Some(id) => Some(id),
                        None => return Ok(Err(AgentError::NoSuchPane(pane.to_owned()))),
                    },
                    (None, None) => None,
                };
                let agents = store::list_agents(conn, args.include_dead)?
                    .into_iter()
                    .filter(|agent| scope.as_ref().is_none_or(|id| id == &agent.workspace_id))
                    .map(|agent| {
                        let started = store::has_session(conn, &agent.id)?;
                        Ok(view(agent, started))
                    })
                    .collect::<rusqlite::Result<_>>()?;
                Ok(Ok(AgentList {
                    agents,
                    revision: store::revision(conn)?,
                }))
            },
        )
        .await?
}

/// `agent.pane_exited`: a pane's process exited, so whatever agent ran in it
/// is over (PRD §9.2).
pub async fn pane_exited(
    state: &AppState,
    args: PaneExitedArgs,
) -> Result<EventOutcome, AgentError> {
    let now = clock::now_millis();
    let ended = state
        .db
        .call(move |conn| store::end_live_in_pane(conn, &args.pane, None, now))
        .await?;
    Ok(EventOutcome { applied: ended > 0 })
}

/// `agent.sweep`: the watchdog. Agents whose pane is gone are ended; running
/// agents with no hook event and no pane output for two minutes become
/// `unknown`, which covers every way hook delivery can fail. Announces a
/// change only when it made one.
pub async fn sweep(state: &AppState) -> Result<EventOutcome, AgentError> {
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
            logic::is_silent(agent.status, agent.last_event_at, output, now, WATCHDOG_MS)
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

fn view(agent: Agent, started: bool) -> AgentView {
    AgentView {
        id: agent.id,
        pane_id: agent.pane_id,
        workspace_id: agent.workspace_id,
        label: agent.label,
        backend: agent.backend,
        status: agent.status,
        status_detail: agent.status_detail,
        status_at: agent.status_at,
        permission_mode: agent.permission_mode,
        task_brief: agent.task_brief,
        started,
        parent_id: agent.parent_id,
        depth: agent.depth,
        started_at: agent.started_at,
        ended_at: agent.ended_at,
    }
}
