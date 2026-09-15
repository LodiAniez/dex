//! Domain types owned by the workspace slice. Wire types live in dex-protocol.

use thiserror::Error;

use crate::platform::db::DbError;

/// A row of the `workspace` table (timestamps are written, never read back).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// Workspace id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Root directory, forward slashes.
    pub root_path: String,
    /// `#rrggbb`, or none.
    pub color: Option<String>,
    /// Sidebar position.
    pub sort_index: i64,
    /// Serialized `Layout`.
    pub layout_json: String,
    /// The pane that had focus.
    pub active_pane: Option<String>,
}

/// A row of the `pane` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    /// Pane id.
    pub id: String,
    /// Owning workspace.
    pub workspace_id: String,
    /// Optional label, unique within the workspace.
    pub label: Option<String>,
    /// Working directory, forward slashes.
    pub cwd: String,
    /// `terminal`, `markdown`, `diff`, or `activity`.
    pub kind: String,
    /// `windows` or `wsl:<distro>`.
    pub runtime: String,
}

/// Everything that can go wrong in the workspace slice. Each variant gets its
/// protocol code and repair string in `router.rs`.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    /// No workspace has this id.
    #[error("no workspace with id {0}")]
    NoSuchWorkspace(String),
    /// Not a `#rrggbb` color.
    #[error("{0:?} is not a color; expected #rrggbb")]
    InvalidColor(String),
    /// Empty or over-long name.
    #[error("workspace names must be 1 to 64 characters")]
    InvalidName,
    /// A reorder that did not list every workspace exactly once.
    #[error("the new order must list every workspace exactly once")]
    InvalidOrder,
    /// The root is not an existing absolute directory.
    #[error("{0:?} is not an existing directory")]
    InvalidRoot(String),
    /// A layout could not be serialized.
    #[error("layout: {0}")]
    Layout(#[from] serde_json::Error),
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
}
