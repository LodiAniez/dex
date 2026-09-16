//! The commands that write to and read the append-only log rather than the
//! key-value entries: notes, directed messages, and the activity stream
//! (docs/prd.md §10.1, §10.4).

use dex_protocol::context::{
    Appended, EventList, EventView, Inbox, Message, MessageArgs, NoteArgs, ScopeArgs,
};

use super::commands::{author, log, scope};
use super::logic;
use super::model::{ContextError, NewEvent};
use super::store;
use crate::app::AppState;
use crate::features::agent;
use crate::platform::clock;

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
pub async fn message_send(state: &AppState, args: MessageArgs) -> Result<Appended, ContextError> {
    let now = clock::now_millis();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Appended, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let Some(target) = agent::resolve_agent(conn, &args.target_agent)? else {
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
                        target_agent: Some(target),
                        created_at: now,
                    },
                )?;
                Ok(Ok(Appended { seq }))
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
