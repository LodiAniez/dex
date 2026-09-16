//! `context.digest`: gathering what an agent is told, and deciding whether to
//! tell it at all (docs/prd.md §10.3). The wording and the budget are pure and
//! live in `digest.rs`; the rate limit lives here, in the daemon, so that a
//! hook cannot skip it.

use dex_protocol::context::{Digest, DigestArgs};

use super::commands::scope;
use super::digest::{self, Change, Orientation, Sibling};
use super::model::ContextError;
use super::store;
use crate::app::AppState;
use std::path::Path;

use crate::features::agent;
use crate::features::repo;
use crate::features::workspace;
use crate::platform::clock;

/// At most one `PostToolBatch` delta per agent per this long (PRD §13 config).
const DELTA_MIN_INTERVAL_MS: i64 = 60_000;
/// And only when at least this many new events exist.
const DELTA_MIN_EVENTS: usize = 1;
/// Most events considered for one delta; the budget cuts it down further.
const DELTA_SCAN: u32 = 500;
/// Most entry keys named in a full digest.
const FULL_ENTRIES: usize = 25;

/// `context.digest`: the text to inject, or `None` when there is nothing to say.
///
/// `None` means the hook prints nothing at all — not "no updates" — because
/// this runs on every prompt of every agent (PRD §10.3).
pub async fn digest(state: &AppState, args: DigestArgs) -> Result<Digest, ContextError> {
    let now = clock::now_millis();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Digest, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let me = scope.agent_id.as_deref();
                let text = if args.kind == "full" {
                    let cap = args.max_chars.unwrap_or(digest::FULL_CAP);
                    let orientation = Orientation {
                        workspace: workspace::workspace_name(conn, &scope.workspace_id)?
                            .unwrap_or_else(|| "this workspace".into()),
                        repos: repo::workspace_repos(conn, &scope.workspace_id)?
                            .into_iter()
                            .map(|(name, worktree, branch)| {
                                // The branch recorded when the worktree was made
                                // can be stale; what is checked out now wins.
                                let live = worktree
                                    .as_deref()
                                    .and_then(|path| repo::branch_at(Path::new(path)));
                                (name, live.or(branch))
                            })
                            .collect(),
                        task_brief: match me {
                            Some(id) => agent::brief_of(conn, id)?,
                            None => None,
                        },
                        siblings: agent::siblings(conn, &scope.workspace_id, me)?
                            .into_iter()
                            .map(|(label, status, task)| Sibling {
                                label,
                                status,
                                task,
                            })
                            .collect(),
                        entries: store::list_entries(conn, &scope.workspace_id)?
                            .into_iter()
                            .take(FULL_ENTRIES)
                            .map(|entry| entry.key)
                            .collect(),
                        unread: match me {
                            Some(id) => store::unread_messages(conn, id)?.len(),
                            None => 0,
                        },
                    };
                    // A session starting has just been told everything current,
                    // so its first delta must not repeat it. The rate-limit
                    // clock is deliberately left alone (0 keeps the stored
                    // value): a full digest is not a `PostToolBatch` delta, and
                    // starting the clock here would mute an autonomous agent
                    // for the first minute of its life.
                    if let Some(id) = me {
                        let head = store::max_seq(conn, &scope.workspace_id)?;
                        store::advance_cursor(conn, id, head, 0)?;
                    }
                    Some(digest::full(&orientation, cap))
                } else {
                    delta_for(conn, &scope.workspace_id, me, &args, now)?
                };
                Ok(Ok(Digest { text }))
            },
        )
        .await?
}

/// The delta half: cursor, rate limit, and the events themselves.
fn delta_for(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    me: Option<&str>,
    args: &DigestArgs,
    now: i64,
) -> rusqlite::Result<Option<String>> {
    // Without an identity there is no cursor to advance, and re-sending the
    // same events every turn would be worse than sending none.
    let Some(me) = me else {
        return Ok(None);
    };
    let (last_seq, last_delta_at) = store::cursor(conn, me)?;
    if args.rate_limited && now - last_delta_at < DELTA_MIN_INTERVAL_MS {
        return Ok(None);
    }
    let events = store::events_since(conn, workspace_id, last_seq, Some(me), DELTA_SCAN)?;
    if events.len() < DELTA_MIN_EVENTS {
        return Ok(None);
    }
    let head = events
        .iter()
        .map(|event| event.seq)
        .max()
        .unwrap_or(last_seq);

    // A directed message is announced, never quoted: only `message_inbox`
    // delivers bodies, so a skimmed digest cannot silently consume one.
    let mut waiting = 0;
    let mut changes = Vec::new();
    for event in &events {
        if event.kind == "message" {
            if event.target_agent.as_deref() == Some(me) {
                waiting += 1;
            }
            continue;
        }
        // Status changes belong to the activity pane, not to another agent's
        // context: a sibling going idle and running several times a turn would
        // spend the delta budget saying nothing actionable.
        if event.kind == "status" {
            continue;
        }
        changes.push(Change {
            author: author(conn, event.agent_id.as_deref())?,
            kind: event.kind.clone(),
            key: event.key.clone(),
            body: event.body.clone(),
            created_at: event.created_at,
        });
    }
    let cap = args.max_chars.unwrap_or(digest::DELTA_CAP);
    let mut text = digest::delta(&changes, now, cap);
    if waiting > 0 {
        let note = format!(
            "\n- {waiting} new directed {}, readable with message_inbox.",
            if waiting == 1 { "message" } else { "messages" }
        );
        text = Some(match text {
            Some(mut body) => {
                body.push_str(&note);
                body
            }
            None => format!("## Workspace context{note}"),
        });
    }
    // The cursor advances whether or not anything was rendered: these events
    // have now been accounted for, and replaying them would double-report.
    //
    // Only a rate-limited delta starts the clock. A `UserPromptSubmit` delta
    // must not, or a human typing would mute the agent's own `PostToolBatch`
    // deltas for a minute; the cursor alone already stops it being told twice.
    let clock = if args.rate_limited { now } else { 0 };
    store::advance_cursor(conn, me, head, clock)?;
    Ok(text)
}

fn author(conn: &rusqlite::Connection, agent_id: Option<&str>) -> rusqlite::Result<String> {
    Ok(match agent_id {
        Some(id) => agent::label_of(conn, id)?.unwrap_or_else(|| "an agent".into()),
        None => "the human".into(),
    })
}
