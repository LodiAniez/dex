//! Wire types for workspaces and panes (docs/prd.md §5, §7.2).
//!
//! Every mutating `workspace.*` command answers with the full `WorkspaceList`,
//! so a client replaces its copy of the state instead of patching it.

use serde::{Deserialize, Serialize};

/// Direction a split node divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SplitDir {
    /// Side by side.
    Horizontal,
    /// Stacked.
    Vertical,
}

/// A workspace's pane layout: the binary tree stored as `layout_json` (PRD §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layout {
    /// A single pane.
    Leaf {
        /// The pane shown here.
        pane_id: String,
    },
    /// Two layouts sharing the space.
    Split {
        /// Split direction.
        dir: SplitDir,
        /// `a`'s share of the space, 0.1 to 0.9.
        ratio: f64,
        /// First child.
        a: Box<Layout>,
        /// Second child.
        b: Box<Layout>,
    },
}

/// A pane as clients see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PaneView {
    /// Pane id.
    pub id: String,
    /// User- or agent-assigned label, unique within the workspace.
    pub label: Option<String>,
    /// Working directory, forward slashes.
    pub cwd: String,
    /// `terminal`, `markdown`, `diff`, or `activity`.
    pub kind: String,
    /// `windows` or `wsl:<distro>`.
    pub runtime: String,
}

/// A workspace and its panes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WorkspaceView {
    /// Workspace id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// `#rrggbb`, or none.
    pub color: Option<String>,
    /// Root directory, forward slashes; hosts `.dex/`.
    pub root_path: String,
    /// Position in the sidebar, ascending.
    pub sort_index: u32,
    /// The pane tree; `None` only if the workspace has no panes.
    pub layout: Option<Layout>,
    /// The pane that had focus.
    pub active_pane: Option<String>,
    /// Every pane, oldest first.
    pub panes: Vec<PaneView>,
}

/// Result of every `workspace.*` command: all workspaces in sidebar order,
/// and which one is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WorkspaceList {
    /// Workspaces in sidebar order.
    pub workspaces: Vec<WorkspaceView>,
    /// Id of the active workspace; `None` only when there are no workspaces.
    pub active: Option<String>,
    /// Grows with every change to the daemon's state. Responses can arrive
    /// out of order, so a client keeps whichever snapshot has the highest.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
}

/// Args for `workspace.create`. All optional: a bare create gets the next
/// default name, the home directory, and the next palette color.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspaceArgs {
    /// Display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Absolute path of an existing directory.
    #[serde(default)]
    pub root_path: Option<String>,
    /// `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
}

/// Args naming one workspace (`workspace.switch`, `workspace.delete`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceArgs {
    /// Workspace id.
    pub workspace: String,
}

/// Args for `workspace.rename`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameWorkspaceArgs {
    /// Workspace id.
    pub workspace: String,
    /// New name, 1–64 characters.
    pub name: String,
}

/// Args for `workspace.recolor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecolorWorkspaceArgs {
    /// Workspace id.
    pub workspace: String,
    /// `#rrggbb`, or `None` to clear.
    #[serde(default)]
    pub color: Option<String>,
}

/// Args for `workspace.reorder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReorderWorkspacesArgs {
    /// Every workspace id exactly once, in the new sidebar order.
    pub order: Vec<String>,
}

/// Args for `workspace.set_layout`: the same panes in a new arrangement.
/// Ratios outside 0.1–0.9 are clamped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetLayoutArgs {
    /// Workspace id.
    pub workspace: String,
    /// The new tree; must show exactly the workspace's panes.
    pub layout: Layout,
}

/// Where `pane.split` puts the new pane.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    /// To the right of the pane.
    #[default]
    Right,
    /// Below the pane.
    Down,
}

/// Args for `pane.split`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPaneArgs {
    /// The pane to split.
    pub pane: String,
    /// Where the new pane goes; right by default.
    #[serde(default)]
    pub direction: SplitDirection,
    /// Working directory; the split pane's by default.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Label for the new pane.
    #[serde(default)]
    pub label: Option<String>,
}

/// Args naming one pane (`pane.close`, `pane.focus`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneArgs {
    /// Pane id.
    pub pane: String,
}

/// Args for `pane.swap`: two panes of the same workspace trade places.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapPanesArgs {
    /// One pane.
    pub a: String,
    /// The other pane.
    pub b: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_documented_json_shape() {
        let leaf = Layout::Leaf {
            pane_id: "p".into(),
        };
        assert_eq!(
            serde_json::to_string(&leaf).unwrap(),
            r#"{"type":"leaf","pane_id":"p"}"#
        );
        let split: Layout = serde_json::from_str(
            r#"{"type":"split","dir":"horizontal","ratio":0.5,
                "a":{"type":"leaf","pane_id":"a"},"b":{"type":"leaf","pane_id":"b"}}"#,
        )
        .unwrap();
        assert!(matches!(
            split,
            Layout::Split {
                dir: SplitDir::Horizontal,
                ..
            }
        ));
    }

    #[test]
    fn bare_create_args_parse_from_an_empty_object() {
        let args: CreateWorkspaceArgs = serde_json::from_str("{}").unwrap();
        assert_eq!(args, CreateWorkspaceArgs::default());
    }
}
