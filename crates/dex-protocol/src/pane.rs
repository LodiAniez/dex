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
    /// Where its shell runs: `windows` or `wsl:<distro>`.
    #[serde(default = "windows")]
    pub runtime: String,
    /// Whether this is its workspace's focused pane.
    pub focused: bool,
    /// Whether its workspace is the one on screen.
    pub in_active_workspace: bool,
}

fn windows() -> String {
    "windows".into()
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
    /// Working directory; the split pane's by default. For a `markdown` pane,
    /// the file to show, which is required.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Label for the new pane.
    #[serde(default)]
    pub label: Option<String>,
    /// `terminal` (the default), `activity`, `diff`, or `markdown`. A
    /// non-terminal pane runs no shell.
    #[serde(default)]
    pub kind: Option<String>,
}

/// Result of `pane.terminal`: the terminal new panes open in, and the choices.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalView {
    /// `windows` (PowerShell, or the configured shell) or `wsl:<distro>`.
    pub terminal: String,
    /// `windows` first, then `wsl:<distro>` for each installed distro.
    pub runtimes: Vec<String>,
    /// After a choice: panes whose shell is running in another terminal. They
    /// keep it unless the owner has them restarted (`pane.started` records
    /// where each one then starts).
    #[serde(default)]
    pub running_elsewhere: Vec<RunningPane>,
}

/// A pane whose shell runs in another terminal than the one chosen.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningPane {
    /// Pane id.
    pub pane: String,
    /// Its workspace's name.
    pub workspace: String,
    /// Its label, if it has one.
    pub label: Option<String>,
    /// The folder it restarts in.
    pub cwd: String,
    /// Where its shell runs now.
    pub runtime: String,
    /// Whether something may be running in it: a restart would end it. Dex
    /// cannot always tell; anything it cannot rule out counts as busy.
    pub busy: bool,
}

/// Args for `pane.started`: a pane's shell has started, in `runtime`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneStartedArgs {
    /// Pane id.
    pub pane: String,
    /// `windows` or `wsl:<distro>`: where the shell is running.
    pub runtime: String,
}

/// Args for `pane.terminal`: with `terminal`, choose it; without, only ask.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalArgs {
    /// `windows` or `wsl:<distro>`, for an installed distro.
    #[serde(default)]
    pub terminal: Option<String>,
}

/// Args for `pane.content`: what a `markdown` pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneContentArgs {
    /// Target pane; must be a `markdown` pane.
    pub pane: String,
}

/// Result of `pane.content`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PaneContent {
    /// The file shown, forward slashes.
    pub path: String,
    /// Its text, or as much of it as Dex will show.
    pub text: String,
    /// Whether `text` was cut short.
    pub truncated: bool,
    /// When the file last changed, unix millis; 0 if unknown.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub modified_at: i64,
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
