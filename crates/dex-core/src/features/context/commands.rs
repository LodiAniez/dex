//! One handler per `context.*` command (docs/prd.md §10.1).
//!
//! Every command resolves a `Caller` first: context is workspace-scoped, and
//! agents in different workspaces are fully isolated, so a command that cannot
//! name a workspace fails rather than guessing one.

use std::path::Path;

use dex_protocol::context::{
    Caller, EntryList, EntrySummary, EntryView, ListArgs, ReadArgs, SearchArgs, SearchHit,
    SearchResults, WriteArgs, Written,
};
use rusqlite::Connection;

use super::logic;
use super::mirror;
use super::model::{ContextError, Entry, Event, NewEvent};
use super::store;
use crate::app::AppState;
use crate::features::{agent, workspace};
use crate::platform::clock;

/// Most hits `context.search` returns when the caller does not say.
const DEFAULT_SEARCH_LIMIT: u32 = 10;

/// Where a command runs: the workspace, its root on disk, and who is asking.
pub(super) struct Scope {
    pub workspace_id: String,
    pub root: String,
    pub agent_id: Option<String>,
}

impl Scope {
    fn root(&self) -> &Path {
        Path::new(&self.root)
    }
}

/// Resolves the caller to a workspace and an agent identity.
///
/// The workspace comes from an explicit argument, else the caller's pane. The
/// agent is `DEX_AGENT_ID` when Dex spawned it, else whatever agent is live in
/// that pane — so a human-started `claude` still writes under its own name.
pub(super) fn scope(
    conn: &Connection,
    caller: &Caller,
) -> rusqlite::Result<Result<Scope, ContextError>> {
    let workspace_id = match (&caller.workspace, &caller.pane) {
        (Some(target), _) => match workspace::workspace_id(conn, target)? {
            Ok(id) => id,
            Err(err) => return Ok(Err(err.into())),
        },
        (None, Some(pane)) => match workspace::find_pane_workspace(conn, pane)? {
            Some(id) => id,
            None => {
                return Ok(Err(
                    workspace::WorkspaceError::NoSuchPane(pane.clone()).into()
                ));
            }
        },
        (None, None) => return Ok(Err(ContextError::NoWorkspace)),
    };
    let Some(root) = workspace::workspace_root(conn, &workspace_id)? else {
        return Ok(Err(ContextError::NoWorkspace));
    };
    let agent_id = match (&caller.agent, &caller.pane) {
        (Some(id), _) => Some(id.clone()),
        (None, Some(pane)) => agent::resolve_agent(conn, pane, Some(&workspace_id))?,
        (None, None) => None,
    };
    Ok(Ok(Scope {
        workspace_id,
        root,
        agent_id,
    }))
}

/// `context.read`: one entry by key.
pub async fn read(state: &AppState, args: ReadArgs) -> Result<EntryView, ContextError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<EntryView, ContextError>> {
                if let Err(bad) = logic::check_key(&args.key) {
                    return Ok(Err(ContextError::InvalidKey {
                        key: args.key,
                        reason: bad,
                    }));
                }
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                match store::find_entry(conn, &scope.workspace_id, &args.key)? {
                    Some(entry) => Ok(Ok(view(conn, entry)?)),
                    None => Ok(Err(ContextError::NoSuchKey(args.key))),
                }
            },
        )
        .await?
}

/// `context.write`: store a fact, with optimistic concurrency (PRD §10.1).
///
/// `expected_version` omitted is last-write-wins and creates the key; `0`
/// means create-only; `N` must match what is stored. The conflict carries the
/// current value so the caller can merge without a second round trip.
pub async fn write(state: &AppState, args: WriteArgs) -> Result<Written, ContextError> {
    let now = clock::now_millis();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<Written, ContextError>> {
                if let Err(bad) = logic::check_key(&args.key) {
                    return Ok(Err(ContextError::InvalidKey {
                        key: args.key,
                        reason: bad,
                    }));
                }
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let tags = logic::clean_tags(args.tags.as_deref());
                let existing = store::find_entry(conn, &scope.workspace_id, &args.key)?;
                let version = match (existing, args.expected_version) {
                    (Some(entry), Some(expected)) if entry.version != expected => {
                        return Ok(Err(ContextError::VersionConflict {
                            key: args.key,
                            expected,
                            current: entry.version,
                            value: entry.value,
                        }));
                    }
                    (Some(entry), _) => store::update_entry(
                        conn,
                        entry.id,
                        &args.value,
                        scope.agent_id.as_deref(),
                        tags.as_deref(),
                        now,
                    )?,
                    // `expected_version: 0` says "create only", and there is nothing
                    // here, so creating is exactly right.
                    (None, _) => store::insert_entry(
                        conn,
                        &scope.workspace_id,
                        &args.key,
                        &args.value,
                        scope.agent_id.as_deref(),
                        tags.as_deref(),
                        now,
                    )?,
                };
                mirror::write_entry(scope.root(), &args.key, &args.value);
                log(
                    conn,
                    &scope,
                    NewEvent {
                        workspace_id: scope.workspace_id.clone(),
                        agent_id: scope.agent_id.clone(),
                        kind: "write",
                        key: Some(args.key.clone()),
                        body: format!("wrote {} (v{version})", args.key),
                        target_agent: None,
                        created_at: now,
                    },
                )?;
                Ok(Ok(Written { version }))
            },
        )
        .await?
}

/// `context.list`: keys and metadata, no values — a cheap survey.
pub async fn list(state: &AppState, args: ListArgs) -> Result<EntryList, ContextError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<EntryList, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let mut entries = Vec::new();
                for entry in store::list_entries(conn, &scope.workspace_id)? {
                    if args
                        .tag
                        .as_deref()
                        .is_some_and(|tag| !logic::has_tag(entry.tags.as_deref(), tag))
                    {
                        continue;
                    }
                    entries.push(EntrySummary {
                        key: entry.key,
                        author: author(conn, entry.author_agent.as_deref())?,
                        tags: entry.tags,
                        updated_at: entry.updated_at,
                    });
                }
                Ok(Ok(EntryList { entries }))
            },
        )
        .await?
}

/// `context.search`: full-text search over this workspace's entries.
pub async fn search(state: &AppState, args: SearchArgs) -> Result<SearchResults, ContextError> {
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<SearchResults, ContextError>> {
                let scope = match scope(conn, &args.caller)? {
                    Ok(scope) => scope,
                    Err(err) => return Ok(Err(err)),
                };
                let limit = args.limit.unwrap_or(DEFAULT_SEARCH_LIMIT).clamp(1, 100);
                let hits = store::search_entries(conn, &scope.workspace_id, &args.query, limit)?
                    .into_iter()
                    .map(|(entry, score)| SearchHit {
                        key: entry.key,
                        value: entry.value,
                        score,
                    })
                    .collect();
                Ok(Ok(SearchResults { hits }))
            },
        )
        .await?
}

/// Appends to the log and mirrors the line to `.dex/activity.log`.
pub(super) fn log(conn: &Connection, scope: &Scope, event: NewEvent) -> rusqlite::Result<i64> {
    let seq = store::insert_event(conn, &event)?;
    mirror::append_event(
        scope.root(),
        &Event {
            seq,
            workspace_id: event.workspace_id,
            agent_id: event.agent_id,
            kind: event.kind.to_owned(),
            key: event.key,
            body: event.body,
            target_agent: event.target_agent,
            read_at: None,
            created_at: event.created_at,
        },
    );
    Ok(seq)
}

pub(super) fn author(
    conn: &Connection,
    agent_id: Option<&str>,
) -> rusqlite::Result<Option<String>> {
    match agent_id {
        Some(id) => agent::label_of(conn, id),
        None => Ok(None),
    }
}

fn view(conn: &Connection, entry: Entry) -> rusqlite::Result<EntryView> {
    Ok(EntryView {
        key: entry.key,
        value: entry.value,
        version: entry.version,
        author: author(conn, entry.author_agent.as_deref())?,
        tags: entry.tags,
        updated_at: entry.updated_at,
    })
}
