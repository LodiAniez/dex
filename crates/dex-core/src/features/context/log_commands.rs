//! The commands that write to and read the append-only log rather than the
//! key-value entries: notes, directed messages, and the activity stream
//! (docs/prd.md §10.1, §10.4).

use dex_protocol::agent::AgentStatus;
use dex_protocol::context::{
    Appended, ClearEventsArgs, ClearScope, Cleared, DeleteEventArgs, Delivery, EventList,
    EventView, Inbox, Message, MessageArgs, NoteArgs, ScopeArgs, Sent,
};

use super::commands::{author, log, scope};
use super::logic;
use super::model::{ContextError, NewEvent};
use super::store;
use crate::app::AppState;
use crate::features::agent;
use crate::platform::clock;

/// What `message_send` settles in the database: where the message landed, how
/// it reaches the recipient, and the wake to type if it must be woken.
type Delivered = (i64, Delivery, Option<super::Wake>);

/// How the message will reach this agent, and the wake to type if it is to be
/// woken now. At its prompt: woken. Busy: it sees the message at its next
/// turn, and the watchdog wakes it after that turn if it has still not read it
/// (`agent::wake_waiting`). Which statuses may be typed at is the agent
/// slice's rule (`agent::waits_at_its_prompt`), asked once here and once
/// there rather than written down twice.
fn delivery_for(
    conn: &rusqlite::Connection,
    target: &str,
) -> rusqlite::Result<(Delivery, Option<super::Wake>)> {
    // Gone, or never there: the message stays in the log, but the sender is
    // not left waiting for an answer to it.
    let who: Option<agent::Whereabouts> = agent::whereabouts(conn, target)?;
    let Some(who) = who else {
        return Ok((Delivery::Ended, None));
    };
    if who.status == AgentStatus::Dead {
        return Ok((Delivery::Ended, None));
    }
    if !agent::waits_at_its_prompt(who.status) {
        return Ok((Delivery::NextTurn, None));
    }
    // At its prompt to Claude Code, but the pane holds a question for the
    // owner: the nudge would be typed as their answer.
    if who.asked_owner {
        return Ok((Delivery::WaitingOnOwner, None));
    }
    let Some(pane) = who.pane_id else {
        return Ok((Delivery::NextTurn, None));
    };
    Ok(
        match super::take_wake(conn, target, pane, who.started, who.idle_at)? {
            Some(wake) => (Delivery::Woken, Some(wake)),
            None => (Delivery::NextTurn, None),
        },
    )
}

/// Most events the activity pane asks for at once.
const DEFAULT_EVENT_LIMIT: u32 = 200;

/// For the agent slice: records a status change in the workspace log, so the
/// activity pane shows what the agents are doing and not only what they say
/// (PRD §10.1). Best-effort — a status change is not worth failing a hook over.
///
/// These rows are deliberately left out of delta digests (`digest_commands`):
/// they are for the human watching the stream, and a sibling changing status
/// several times a turn would eat the delta budget without telling an agent
/// anything it needs.
pub fn record_status(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    agent_id: &str,
    body: String,
    now: i64,
) -> rusqlite::Result<()> {
    record_event(conn, workspace_id, Some(agent_id), "status", body, now)
}

/// For other slices: puts one event in the workspace log.
///
/// The kind is a `&'static str` so only kinds this codebase knows about can be
/// written; the schema's list is the closed set (PRD §5).
pub fn record_event(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    agent_id: Option<&str>,
    kind: &'static str,
    body: String,
    now: i64,
) -> rusqlite::Result<()> {
    store::insert_event(
        conn,
        &NewEvent {
            workspace_id: workspace_id.to_owned(),
            agent_id: agent_id.map(str::to_owned),
            kind,
            key: None,
            body,
            target_agent: None,
            created_at: now,
        },
    )?;
    Ok(())
}

/// `context.note`: append a freeform line to the workspace log.
pub async fn note(state: &AppState, args: NoteArgs) -> Result<Appended, ContextError> {
    let now = clock::now_millis();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Appended, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let seq = log(
                    conn,
                    &scope,
                    NewEvent {
                        workspace_id: scope.workspace_id.clone(),
                        agent_id: scope.agent_id.clone(),
                        kind: "note",
                        key: logic::clean_tags(args.tags.as_deref()),
                        body: args.body,
                        target_agent: None,
                        created_at: now,
                    },
                )?;
                Ok(Ok(Appended { seq }))
            },
        )
        .await?
}

/// `context.message_send`: a directed note to one sibling agent.
pub async fn message_send(state: &AppState, args: MessageArgs) -> Result<Sent, ContextError> {
    let now = clock::now_millis();
    let (seq, delivery, wake) = state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Delivered, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                // Within the sender's own workspace: a label names one agent
                // there, and may name another somewhere else (issue #59).
                let found =
                    agent::resolve_agent(conn, &args.target_agent, Some(&scope.workspace_id))?;
                let Some(target) = found else {
                    return Ok(Err(ContextError::NoSuchAgent(args.target_agent)));
                };
                let seq = log(
                    conn,
                    &scope,
                    NewEvent {
                        workspace_id: scope.workspace_id.clone(),
                        agent_id: scope.agent_id.clone(),
                        kind: "message",
                        key: None,
                        body: args.body,
                        target_agent: Some(target.clone()),
                        created_at: now,
                    },
                )?;
                let (delivery, wake) = delivery_for(conn, &target)?;
                Ok(Ok((seq, delivery, wake)))
            },
        )
        .await??;
    // What the sender is told is what happened: a pane whose shell has gone
    // cannot be typed into, and then the message waits for a turn like any
    // other.
    let delivery = match wake {
        Some(wake) => match super::wake_up(state, wake).await {
            true => delivery,
            false => Delivery::NextTurn,
        },
        None => delivery,
    };
    Ok(Sent { seq, delivery })
}

/// `context.delete_event`: removes one event from the workspace log.
///
/// The log is a record for the human watching, not an audit trail; an event
/// that is noise to them is theirs to remove. Digest cursors are sequence
/// numbers, so a gap in the sequence costs nothing.
pub async fn delete_event(
    state: &AppState,
    args: DeleteEventArgs,
) -> Result<Cleared, ContextError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Cleared, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                if !store::delete_event(conn, &scope.workspace_id, args.seq)? {
                    return Ok(Err(ContextError::NoSuchEvent(args.seq)));
                }
                Ok(Ok(Cleared { removed: 1 }))
            },
        )
        .await?
}

/// `context.clear_events`: removes the events of agents that have ended, or
/// every event in the workspace.
///
/// "Ended" is decided by the agent slice, not by a join here: which agents
/// are dead is its knowledge. The human's own events (no agent) and live
/// agents' events survive an `ended` clear, so what is still happening stays
/// visible while what is over goes.
pub async fn clear_events(
    state: &AppState,
    args: ClearEventsArgs,
) -> Result<Cleared, ContextError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Cleared, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let removed = match args.scope {
                    ClearScope::All => store::delete_all_events(conn, &scope.workspace_id)?,
                    ClearScope::Ended => {
                        let ended = agent::ended_in_workspace(conn, &scope.workspace_id)?;
                        store::delete_events_by(conn, &scope.workspace_id, &ended)?
                    }
                };
                Ok(Ok(Cleared {
                    removed: removed as u64,
                }))
            },
        )
        .await?
}

/// `context.inbox`: unread directed messages, which reading marks read.
///
/// This is the only path that delivers message bodies; digests show a count
/// only, so a message is never silently consumed by a digest the agent skims.
pub async fn inbox(state: &AppState, args: ScopeArgs) -> Result<Inbox, ContextError> {
    let now = clock::now_millis();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Inbox, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let Some(me) = scope.agent_id.as_deref() else {
                    return Ok(Ok(Inbox {
                        messages: Vec::new(),
                    }));
                };
                let unread = store::unread_messages(conn, me)?;
                let mut messages = Vec::new();
                for event in &unread {
                    messages.push(Message {
                        seq: event.seq,
                        from: author(conn, event.agent_id.as_deref())?,
                        body: event.body.clone(),
                        created_at: event.created_at,
                    });
                }
                let seqs: Vec<i64> = unread.iter().map(|event| event.seq).collect();
                store::mark_read(conn, &seqs, now)?;
                Ok(Ok(Inbox { messages }))
            },
        )
        .await?
}

/// `context.events`: the workspace's recent log, for the activity pane.
pub async fn events(state: &AppState, args: ScopeArgs) -> Result<EventList, ContextError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<EventList, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let limit = args.limit.unwrap_or(DEFAULT_EVENT_LIMIT).clamp(1, 1000);
                let mut events = Vec::new();
                for event in store::list_events(conn, &scope.workspace_id, limit)? {
                    events.push(EventView {
                        seq: event.seq,
                        kind: event.kind,
                        author: author(conn, event.agent_id.as_deref())?,
                        agent_id: event.agent_id,
                        target_agent_id: event.target_agent.clone(),
                        key: event.key,
                        // A directed message's body belongs to its recipient only.
                        body: if event.target_agent.is_some() {
                            format!(
                                "message to {}",
                                author(conn, event.target_agent.as_deref())?
                                    .unwrap_or_else(|| "another agent".into())
                            )
                        } else {
                            event.body
                        },
                        created_at: event.created_at,
                    });
                }
                Ok(Ok(EventList {
                    events,
                    revision: store::revision(conn)?,
                }))
            },
        )
        .await?
}
