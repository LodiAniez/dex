//! Domain types owned by the agent slice. Wire types live in dex-protocol.

use dex_protocol::agent::AgentStatus;
use thiserror::Error;

use crate::features::workspace::WorkspaceError;
use crate::platform::db::DbError;

/// A row of the `agent` table (the columns the daemon reads back).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    /// Agent id.
    pub id: String,
    /// The pane it runs in; `None` once the pane is closed.
    pub pane_id: Option<String>,
    /// Its workspace.
    pub workspace_id: String,
    /// Display label.
    pub label: Option<String>,
    /// `claude`.
    pub backend: String,
    /// Current state.
    pub status: AgentStatus,
    /// Why, for `error`.
    pub status_detail: Option<String>,
    /// Hook stamp of the transition that set `status`.
    pub status_at: i64,
    /// Permission mode as last reported.
    pub permission_mode: Option<String>,
    /// What it was spawned to do.
    pub task_brief: Option<String>,
    /// When it started.
    pub started_at: i64,
    /// When any hook last arrived for it (the watchdog's clock).
    pub last_event_at: i64,
    /// When it ended.
    pub ended_at: Option<i64>,
}

/// Everything that can go wrong in the agent slice. Each variant gets its
/// protocol code and repair string in `router.rs`.
#[derive(Debug, Error)]
pub enum AgentError {
    /// A hook from a pane Dex does not know (closed since, or not a Dex pane).
    #[error("no pane matches {0:?}")]
    NoSuchPane(String),
    /// No agent, and no pane with a live agent, matches the target.
    #[error("no running agent matches {0:?}")]
    NoSuchAgent(String),
    /// The agent has already ended.
    #[error("agent {0} has already ended")]
    NotRunning(String),
    /// A workspace or pane argument did not resolve, or the pane could not be written.
    #[error(transparent)]
    Target(#[from] WorkspaceError),
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
}
