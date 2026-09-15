//! Wire types for the pane commands the CLI uses (docs/prd.md §11).
//!
//! Every `pane` field in these args is a target: a pane id or a pane label.

use serde::{Deserialize, Serialize};

/// One pane in a `pane.list` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneSummary {
    /// Pane id.
    pub id: String,
    /// Id of the pane's workspace.
    pub workspace_id: String,
    /// Name of the pane's workspace.
    pub workspace: String,
    /// The pane's label, if it has one.
    pub label: Option<String>,
    /// Working directory the pane started in, forward slashes.
    pub cwd: String,
    /// `terminal`, `markdown`, `diff`, or `activity`.
    pub kind: String,
    /// Whether this is its workspace's focused pane.
    pub focused: bool,
    /// Whether its workspace is the one on screen.
    pub in_active_workspace: bool,
}

/// Result of `pane.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneList {
    /// Panes in sidebar order, then creation order.
    pub panes: Vec<PaneSummary>,
}

/// Args for `pane.list`: one workspace (id or name), the workspace holding
/// `pane`, or — with neither — every workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPanesArgs {
    /// Workspace id or name.
    #[serde(default)]
    pub workspace: Option<String>,
    /// A pane whose workspace to list.
    #[serde(default)]
    pub pane: Option<String>,
}

/// Args for `pane.create`: a new pane in a workspace — `workspace`, else the
/// one holding `pane`, else the active one — split from its focused pane.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePaneArgs {
    /// Workspace id or name.
    #[serde(default)]
    pub workspace: Option<String>,
    /// A pane whose workspace gets the new pane.
    #[serde(default)]
    pub pane: Option<String>,
    /// Working directory; the split pane's by default.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Label for the new pane.
    #[serde(default)]
    pub label: Option<String>,
}

/// Result of `pane.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Created {
    /// The new pane's id.
    pub pane: String,
}

/// Args for `pane.label`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelPaneArgs {
    /// Target pane.
    pub pane: String,
    /// New label; `None` clears it.
    #[serde(default)]
    pub label: Option<String>,
}

/// Args for `pane.send`: text for the pane's shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendArgs {
    /// Target pane.
    pub pane: String,
    /// Text to type.
    pub text: String,
    /// Press Enter after the text.
    #[serde(default)]
    pub enter: bool,
}

/// Keys `pane.send_key` can press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Key {
    /// Enter.
    Enter,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Ctrl+C.
    CtrlC,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
}

/// Args for `pane.send_key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendKeyArgs {
    /// Target pane.
    pub pane: String,
    /// The key.
    pub key: Key,
}

/// Result of `pane.send` and `pane.send_key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    /// Id of the pane that received the input.
    pub pane: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_use_their_cli_spelling() {
        assert_eq!(serde_json::to_string(&Key::CtrlC).unwrap(), "\"ctrl-c\"");
        let parsed: Key = serde_json::from_str("\"escape\"").unwrap();
        assert_eq!(parsed, Key::Escape);
    }
}
