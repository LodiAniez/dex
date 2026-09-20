//! One handler per `agent.*` command: hook events and pane exits (docs/prd.md
//! §9.1–§9.2). The list is next door in `listing.rs`, the watchdog sweep in
//! `watchdog.rs`.

use dex_protocol::agent::{AgentEventArgs, AgentStatus, EventOutcome, PaneExitedArgs};
use rusqlite::Connection;

use super::identity;
use super::logic::{self, HookInput, HookKind};
use super::model::{Agent, AgentError};
use super::session;
use super::store;
use crate::app::AppState;
use crate::features::{context, workspace};
use crate::platform::{clock, ids};

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
    // /clear: the session ends, the agent does not (a SessionStart follows).
    if kind == HookKind::SessionEnd && !session::ends_the_agent(input.end_reason.as_deref()) {
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
    // /clear or compaction: the same agent carries on, maybe under a new
    // session id, and its status does not change.
    if session::keeps_live_agent(hook.input.source.as_deref())
        && let Some(agent) = store::find_live_in_pane(conn, pane)?
    {
        return store::update_session(conn, &agent.id, hook.session(), hook.mode(), hook.now);
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
    // A session nobody holds may still be the pane's agent carrying on; a
    // spawned agent found by its id carries on through its own /resume even
    // when the end was recorded after this start was stamped.
    let existing = match existing {
        Some(found)
            if found.pane_id.as_deref() == Some(pane.as_str())
                && carries_on_from_end(&found, hook) =>
        {
            Some(bring_back(conn, found, hook)?)
        }
        Some(found) => Some(found),
        None => carried_on(conn, hook)?,
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

/// The pane's agent that ended moments ago, carrying on into a session nobody
/// holds - its SessionEnd came first and ended it. Only when no agent in the
/// pane is live: a start beside a live one may be a new Claude Code after
/// Ctrl+C, which sends no end, and must not take the old one's identity.
fn carried_on(conn: &Connection, hook: &Hook<'_>) -> rusqlite::Result<Option<Agent>> {
    if store::find_live_in_pane(conn, &hook.args.pane)?.is_some() {
        return Ok(None);
    }
    let Some(agent) = store::find_last_ended_in_pane(conn, &hook.args.pane)?.filter(|agent| {
        hook.args.agent.as_ref().is_none_or(|id| *id == agent.id)
            && carries_on_from_end(agent, hook)
    }) else {
        return Ok(None);
    };
    bring_back(conn, agent, hook).map(Some)
}

/// Whether this start is `agent`, ended moments ago, carrying on: timed by the
/// ending hook's stamp (status_at, not the daemon's ended_at) and, for a
/// `resume`, only if it left by `/resume`.
fn carries_on_from_end(agent: &Agent, hook: &Hook<'_>) -> bool {
    let source = hook.input.source.as_deref();
    agent.status == AgentStatus::Dead
        && session::revives_just_ended(source)
        && session::carries_on(agent.status_at, hook.args.stamp)
        && session::ended_for(source, agent.status_detail.as_deref())
}

/// Undoes an agent's end, parking it in `unknown` for the hook to move on.
fn bring_back(conn: &Connection, mut agent: Agent, hook: &Hook<'_>) -> rusqlite::Result<Agent> {
    store::revive(conn, &agent.id, hook.args.stamp)?;
    agent.status = AgentStatus::Unknown;
    agent.status_at = hook.args.stamp;
    agent.ended_at = None;
    Ok(agent)
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
        let told = super::reason::keeps_its_reason(
            hook.kind == HookKind::Waiting,
            agent.status_detail.as_deref(),
        );
        if !told && logic::is_new_wait(agent.status, agent.status_at, hook.kind, hook.args.stamp) {
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
        // A turn that ended by asking the owner something: idle, and waiting on them.
        AgentStatus::Idle if hook.kind == HookKind::Stop => {
            super::asking::idle_detail(&hook.args.input)
        }
        // Left by /resume: a resume moments later may be it carrying on.
        AgentStatus::Dead => {
            session::end_detail(hook.input.end_reason.as_deref()).map(str::to_owned)
        }
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
    let logged = detail
        .as_deref()
        .filter(|why| !super::asking::stays_out_of_the_log(why));
    record_status(conn, agent, hook, next, logged)
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
