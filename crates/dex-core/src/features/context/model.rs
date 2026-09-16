//! Domain types owned by the context slice. Wire types live in dex-protocol.

use thiserror::Error;

use super::logic::BadKey;
use crate::features::workspace::WorkspaceError;
use crate::platform::db::DbError;

/// A row of `context_entry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Rowid; the FTS index is keyed on it.
    pub id: i64,
    /// Its workspace.
    pub workspace_id: String,
    /// Its key.
    pub key: String,
    /// Its value.
    pub value: String,
    /// Bumped on every write.
    pub version: i64,
    /// The agent that wrote it, or `None` for the human.
    pub author_agent: Option<String>,
    /// Comma-separated tags.
    pub tags: Option<String>,
    /// When it last changed.
    pub updated_at: i64,
}

/// A row of `context_event`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Position in the log.
    pub seq: i64,
    /// Its workspace.
    pub workspace_id: String,
    /// The agent responsible, or `None` for the human.
    pub agent_id: Option<String>,
    /// `note`, `write`, `delete`, `message`, `spawn`, or `status`.
    pub kind: String,
    /// The entry key, for `write` and `delete`.
    pub key: Option<String>,
    /// What happened.
    pub body: String,
    /// The recipient, for `message`.
    pub target_agent: Option<String>,
    /// When the recipient read it.
    pub read_at: Option<i64>,
    /// When it happened.
    pub created_at: i64,
}

/// An event on its way into the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    /// Its workspace.
    pub workspace_id: String,
    /// The agent responsible.
    pub agent_id: Option<String>,
    /// Its kind.
    pub kind: &'static str,
    /// The entry key, when there is one.
    pub key: Option<String>,
    /// What happened.
    pub body: String,
    /// The recipient, for `message`.
    pub target_agent: Option<String>,
    /// When it happened.
    pub created_at: i64,
}

/// Everything that can go wrong in the context slice. Each variant gets its
/// protocol code and repair string in `router.rs`.
#[derive(Debug, Error)]
pub enum ContextError {
    /// The key breaks PRD §10.1, or could not be stored on disk.
    #[error("{key:?} is not a usable context key")]
    InvalidKey {
        /// What the caller asked for, so the message names it rather than the
        /// rule it broke: a model reading "Traversal is not a usable key" has
        /// to guess which of its arguments was wrong.
        key: String,
        /// Which rule it broke; `router.rs` turns this into the repair.
        reason: BadKey,
    },
    /// `context.read` on a key that was never written.
    #[error("no entry for key {0:?}")]
    NoSuchKey(String),
    /// A write lost an optimistic-concurrency check.
    #[error(
        "{key:?} is at version {current}, not the expected {expected}; it was changed since you read it"
    )]
    VersionConflict {
        /// The contested key.
        key: String,
        /// What the writer expected.
        expected: i64,
        /// What is actually stored.
        current: i64,
        /// The value now stored, so the caller can merge without re-reading.
        value: String,
    },
    /// `message_send` to an agent that does not exist.
    #[error("no agent matches {0:?}")]
    NoSuchAgent(String),
    /// The caller is not in a pane and named no workspace.
    #[error("this command needs a workspace: it is not running inside a Dex pane")]
    NoWorkspace,
    /// A workspace or pane argument did not resolve.
    #[error(transparent)]
    Target(#[from] WorkspaceError),
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
}
