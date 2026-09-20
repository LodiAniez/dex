//! Wire types for the cross-agent context store (docs/prd.md §10).

use serde::{Deserialize, Serialize};

/// Who is asking and where. Every context command carries one, flattened into
/// its arguments. The workspace comes from the pane unless named outright;
/// the agent is `DEX_AGENT_ID` if the caller has one, else the pane's live agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caller {
    /// `DEX_PANE_ID` of the caller.
    #[serde(default)]
    pub pane: Option<String>,
    /// Workspace id or name, when the caller is not in a pane.
    #[serde(default)]
    pub workspace: Option<String>,
    /// `DEX_AGENT_ID`, for agents Dex spawned.
    #[serde(default)]
    pub agent: Option<String>,
}

/// One stored fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EntryView {
    /// Its key.
    pub key: String,
    /// Its value.
    pub value: String,
    /// Bumped on every write; pass it back as `expected_version` to write safely.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub version: i64,
    /// Label of the agent that wrote it, or `null` when a human did.
    pub author: Option<String>,
    /// Comma-separated tags.
    pub tags: Option<String>,
    /// When it last changed, unix millis.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
}

/// A row of `context_list`: metadata only, so a survey costs no tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EntrySummary {
    /// Its key.
    pub key: String,
    /// Label of the agent that wrote it.
    pub author: Option<String>,
    /// Comma-separated tags.
    pub tags: Option<String>,
    /// When it last changed, unix millis.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
}

/// A search hit. `score` is FTS5 rank, lower being a better match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SearchHit {
    /// Its key.
    pub key: String,
    /// Its value.
    pub value: String,
    /// FTS5 rank; lower is a closer match.
    pub score: f64,
}

/// One line of the append-only log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EventView {
    /// Position in the log; also the delta cursor.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: i64,
    /// `note`, `write`, `delete`, `message`, `spawn`, or `status`.
    pub kind: String,
    /// Label of the agent responsible, or `null` for the human.
    pub author: Option<String>,
    /// Id of that agent, or `null` for the human. Labels can repeat; this is
    /// what to filter on.
    pub agent_id: Option<String>,
    /// For a `message`, the id of the agent it was sent to. Its body stays
    /// the recipient's alone; who it was for does not.
    pub target_agent_id: Option<String>,
    /// The entry key, for `write` and `delete`.
    pub key: Option<String>,
    /// What happened.
    pub body: String,
    /// When, unix millis.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

/// Result of `context.events`, for the activity pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EventList {
    /// Events, newest last.
    pub events: Vec<EventView>,
    /// Grows with every change; a client keeps the snapshot with the highest.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
}

/// An unread directed message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Message {
    /// Position in the log.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: i64,
    /// Label of the sender.
    pub from: Option<String>,
    /// The message.
    pub body: String,
    /// When it was sent, unix millis.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

/// Args for `context.read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadArgs {
    /// The key to read.
    pub key: String,
    #[serde(flatten)]
    pub caller: Caller,
}

/// Args for `context.write` (semantics in PRD §10.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteArgs {
    /// The key to write.
    pub key: String,
    /// Its new value.
    pub value: String,
    /// Comma-separated tags.
    #[serde(default)]
    pub tags: Option<String>,
    /// Omitted: last write wins. `0`: create only. `N`: must match, or conflict.
    #[serde(default)]
    pub expected_version: Option<i64>,
    #[serde(flatten)]
    pub caller: Caller,
}

/// Result of `context.write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    /// The version now stored.
    pub version: i64,
}

/// Args for `context.list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListArgs {
    /// Only entries carrying this tag.
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(flatten)]
    pub caller: Caller,
}

impl ListArgs {
    /// Everything in the caller's workspace, unfiltered.
    pub fn all(caller: Caller) -> Self {
        Self { tag: None, caller }
    }
}

/// Args for `context.search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchArgs {
    /// FTS5 query.
    pub query: String,
    /// Most hits to return (default 10).
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(flatten)]
    pub caller: Caller,
}

/// Args for `context.note`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteArgs {
    /// What to record.
    pub body: String,
    /// Comma-separated tags.
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(flatten)]
    pub caller: Caller,
}

/// Args for `context.message_send`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageArgs {
    /// Agent id or label to send to.
    pub target_agent: String,
    /// The message.
    pub body: String,
    #[serde(flatten)]
    pub caller: Caller,
}

/// Result of an append to the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appended {
    /// Its position in the log.
    pub seq: i64,
}

/// Result of `context.message_send`: where it landed in the log, and whether
/// the recipient was woken to read it (issue #58).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Sent {
    /// Its position in the log.
    pub seq: i64,
    /// Whether the recipient was idle and has been woken to read it. When
    /// false it is working, or waiting on the owner, and reads the message at
    /// its next turn. Defaulted, so an older daemon's reply still parses.
    #[serde(default)]
    pub woken: bool,
}

/// Args for `context.inbox` and `context.events`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeArgs {
    /// Most rows to return.
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(flatten)]
    pub caller: Caller,
}

impl ScopeArgs {
    /// The default window for a caller.
    pub fn for_caller(caller: Caller) -> Self {
        Self {
            limit: None,
            caller,
        }
    }
}

/// Result of `context.inbox`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inbox {
    /// Messages, oldest first. Reading them marks them read.
    pub messages: Vec<Message>,
}

/// Args for `context.digest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestArgs {
    /// `full` on SessionStart, `delta` on every later hook.
    pub kind: String,
    /// Apply the `PostToolBatch` rate limit; `UserPromptSubmit` does not.
    #[serde(default)]
    pub rate_limited: bool,
    /// Override the character cap, for debugging.
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(flatten)]
    pub caller: Caller,
}

/// Result of `context.digest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest {
    /// The text to inject, or `None` when there is nothing worth saying.
    pub text: Option<String>,
}

/// Result of `context.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryList {
    /// Entries, most recently updated first.
    pub entries: Vec<EntrySummary>,
}

/// Result of `context.search`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Hits, best match first.
    pub hits: Vec<SearchHit>,
}

/// Args for `context.delete_event`: one event out of the workspace log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteEventArgs {
    /// The event's `seq`.
    pub seq: i64,
    #[serde(flatten)]
    pub caller: Caller,
}

/// What `context.clear_events` removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClearScope {
    /// Events from agents that have ended. Live agents' and the human's stay.
    Ended,
    /// Every event in the workspace.
    All,
}

/// Args for `context.clear_events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearEventsArgs {
    pub scope: ClearScope,
    #[serde(flatten)]
    pub caller: Caller,
}

/// Result of `context.delete_event` and `context.clear_events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Cleared {
    /// How many events were removed.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub removed: u64,
}
