//! The commands that write to and read the append-only log rather than the
//! key-value entries: notes, directed messages, and the activity stream
//! (docs/prd.md §10.1, §10.4).

use dex_protocol::agent::AgentStatus;
use dex_protocol::context::{
    Appended, ClearEventsArgs, ClearScope, Cleared, DeleteEventArgs, EventList, EventView, Inbox,
    Message, MessageArgs, NoteArgs, ScopeArgs,
};
use dex_protocol::pane::SendArgs;

use super::commands::{author, log, scope};
use super::logic;
use super::model::{ContextError, NewEvent};
use super::store;
use crate::app::AppState;
use crate::features::{agent, workspace};
use crate::platform::clock;

/// Most events the activity pane asks for at once.
const DEFAULT_EVENT_LIMIT: u32 = 200;

/// Typed into an idle agent's pane when a message arrives for it, as a prompt.
///
/// Digests reach an agent only at its next turn, and an agent sitting idle at
/// its prompt has no next turn until someone gives it one. Without this, a
/// message to a finished agent waits forever — which is exactly the case
/// messages exist for: "the requirement changed". This is a prompt, not
/// injected context, so unlike a digest it may say what to do.
const NUDGE: &str = "Another agent has sent you a message. Read it with message_inbox and act on it if it changes your task.";

/// Whether a message just stored for an agent should wake it. Only an idle
/// agent that has actually started: a running one sees the message at its
/// next tool batch, one waiting on a permission prompt would have the text
/// land in that dialog, an ended one has no prompt to type into — and a
/// spawned row is `idle` before Claude Code has started at all, when the pane
/// may be showing the trust dialog, where typed text plus Enter means
/// "No, exit". Only the first unread message: a second nudge would queue a
/// second prompt behind the first.
fn should_nudge(status: AgentStatus, started: bool, unread_after: usize) -> bool {
    started && status == AgentStatus::Idle && unread_after == 1
}

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
    let (appended, nudge) = state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<(Appended, Option<String>), ContextError>> {
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
                        target_agent: Some(target.clone()),
                        created_at: now,
                    },
                )?;
                let nudge = match agent::whereabouts(conn, &target)? {
                    Some(agent::Whereabouts {
                        pane_id: Some(pane),
                        status,
                        started,
                    }) if should_nudge(
                        status,
                        started,
                        store::unread_messages(conn, &target)?.len(),
                    ) =>
                    {
                        Some(pane)
                    }
                    _ => None,
                };
                Ok(Ok((Appended { seq }, nudge)))
            },
        )
        .await??;
    if let Some(pane) = nudge {
        // Best effort, after the message is safely stored: a pane whose shell
        // is not running yet cannot be typed into, and that is not a reason
        // to fail the send — the agent still gets the message at its next turn.
        let typed = workspace::send(
            state,
            SendArgs {
                pane: pane.clone(),
                text: NUDGE.to_owned(),
                enter: true,
            },
        )
        .await;
        if let Err(err) = typed {
            tracing::debug!(%pane, %err, "could not wake the idle agent for its message");
        }
    }
    Ok(appended)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_idle_agent_with_exactly_one_unread_message_is_woken() {
        // The case that motivated this: a finished child, sitting at its
        // prompt, sent a requirement change it would otherwise never see.
        assert!(should_nudge(AgentStatus::Idle, true, 1));
        // A second message while the first is still unread: the first nudge
        // is already queued as a prompt; another would queue another.
        assert!(!should_nudge(AgentStatus::Idle, true, 2));
        // Running: the next tool batch delivers it without typing anything.
        assert!(!should_nudge(AgentStatus::Running, true, 1));
        // Waiting on a permission prompt: typed text would answer that dialog.
        assert!(!should_nudge(AgentStatus::Waiting, true, 1));
        assert!(!should_nudge(AgentStatus::Error, true, 1));
        assert!(!should_nudge(AgentStatus::Dead, true, 1));
        // Spawned but Claude Code has not started: `idle` from birth, and the
        // pane may be showing the trust dialog, where Enter means "No, exit".
        // Seen live: a nudge typed here quit the child.
        assert!(!should_nudge(AgentStatus::Idle, false, 1));
    }

    #[test]
    fn the_nudge_tells_the_agent_how_to_read_the_message() {
        assert!(NUDGE.contains("message_inbox"));
        assert!(!NUDGE.contains('\n'), "one line: it is typed as a prompt");
    }
}
