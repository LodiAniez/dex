//! Wire types for agents: Claude Code sessions running in panes (docs/prd.md §9).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An agent's lifecycle state (PRD §9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Alive, waiting for a human prompt.
    Idle,
    /// Working.
    Running,
    /// Blocked on a human: a permission prompt or a question.
    Waiting,
    /// Stopped by an API failure; `status_detail` says which.
    Error,
    /// Should be running but has gone silent (the watchdog's verdict).
    Unknown,
    /// The session ended or its process exited.
    Dead,
}

/// An agent as clients see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AgentView {
    /// Agent id.
    pub id: String,
    /// The pane it runs in; `None` once that pane is closed.
    pub pane_id: Option<String>,
    /// Its workspace.
    pub workspace_id: String,
    /// Display label.
    pub label: Option<String>,
    /// Messages waiting unread for it: what a sender cannot otherwise see. A
    /// count, not their contents, so it is told to every agent in the
    /// workspace, unlike `status_detail`. Defaulted, so a newer client still
    /// parses an older daemon's reply.
    #[serde(default)]
    pub unread: usize,
    /// `claude`.
    pub backend: String,
    /// Current state.
    pub status: AgentStatus,
    /// Why, for `error` (e.g. `rate_limit`).
    pub status_detail: Option<String>,
    /// When the current state began, unix millis.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub status_at: i64,
    /// When Dex last heard from its hooks, unix millis. What `quiet_for_ms` is
    /// measured from, so a client can go on counting the silence between one
    /// listing and the next.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_event_at: i64,
    /// How long it has been working without a hook, in millis, once that is
    /// long enough to be worth saying (`[agents] quiet_after_seconds`). Hooks
    /// fire at every tool batch, so this is one tool call taking all that
    /// time: a long build, or one that will never come back (issue #67).
    /// Shown, never acted on. `None` for every other status.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub quiet_for_ms: Option<i64>,
    /// Claude Code's permission mode, as its hooks last reported it.
    pub permission_mode: Option<String>,
    /// What it was spawned to do.
    pub task_brief: Option<String>,
    /// Whether Claude Code has started in its pane. A hire exists from the
    /// moment it is made and starts a few seconds later; until then its pane
    /// is a bare shell, and nothing should be typed into it.
    pub started: bool,
    /// The agent that spawned it; `None` for one a human started.
    pub parent_id: Option<String>,
    /// How many spawns separate it from a human: 0 for one a human started.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub depth: i64,
    /// When it started, unix millis.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub started_at: i64,
    /// When it ended, unix millis.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub ended_at: Option<i64>,
}

/// Result of `agent.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AgentList {
    /// Agents, newest first.
    pub agents: Vec<AgentView>,
    /// Grows with every change; a client keeps the snapshot with the highest,
    /// since responses can arrive out of order.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
}

/// Args for `agent.list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListAgentsArgs {
    /// Only this workspace (id or name).
    #[serde(default)]
    pub workspace: Option<String>,
    /// Scope to the workspace holding this pane, when no workspace is named.
    /// A caller inside a pane must not see agents from other workspaces.
    #[serde(default)]
    pub pane: Option<String>,
    /// Include agents whose session has ended.
    #[serde(default)]
    pub include_dead: bool,
}

/// Args for `agent.event`: one Claude Code hook firing in a pane. The hook's
/// stdin JSON goes through untouched; the daemon picks out what it needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEventArgs {
    /// `session-start`, `prompt`, `batch`, `permission`, `waiting`, `idle`,
    /// `stop`, `stop-failure`, or `session-end`. Unknown kinds are ignored.
    pub kind: String,
    /// `DEX_PANE_ID` of the shell the hook ran in.
    pub pane: String,
    /// `DEX_AGENT_ID`, set for agents Dex spawned.
    #[serde(default)]
    pub agent: Option<String>,
    /// Unix millis when `dex event` started: events are applied in this order,
    /// since background hooks can reach the daemon out of order.
    pub stamp: i64,
    /// The hook's stdin JSON.
    #[serde(default)]
    pub input: Value,
}

/// Result of `agent.event`, `agent.pane_exited`, and `agent.sweep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventOutcome {
    /// Whether anything changed.
    pub applied: bool,
}

/// Args for `agent.spawn` (PRD §9.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnArgs {
    /// What the new agent is for. Reaches it through its opening digest, never
    /// through a shell command line.
    pub task: String,
    /// Registered repo to work in.
    #[serde(default)]
    pub repo: Option<String>,
    /// Branch to create a worktree on; requires `repo`.
    #[serde(default)]
    pub worktree: Option<String>,
    /// Label for the new pane.
    #[serde(default)]
    pub label: Option<String>,
    /// `right` or `down`.
    #[serde(default)]
    pub direction: Option<String>,
    /// The pane the caller is in; the new pane is split from it.
    #[serde(default)]
    pub pane: Option<String>,
    /// Workspace, when the caller is not in a pane.
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Result of `agent.spawn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spawned {
    /// The new agent's id, which becomes its `DEX_AGENT_ID`.
    pub agent: String,
    /// The pane it runs in.
    pub pane: String,
    /// Where it is working.
    pub cwd: String,
    /// The branch it is on, when it was given a worktree.
    pub branch: Option<String>,
}

/// Args for `agent.stop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopAgentArgs {
    /// Agent id or label, or the label or id of the pane it runs in.
    pub agent: String,
    /// Ask Claude Code to leave (`/exit`) before resorting to Ctrl+C, so its
    /// `SessionEnd` hook runs. What the office's clock-out asks for.
    #[serde(default)]
    pub graceful: bool,
    /// Close the agent's pane whoever made it. Without this only a pane Dex
    /// made for a spawned agent is closed.
    #[serde(default)]
    pub close_pane: bool,
    /// The pane the request came from, when it came from one (`DEX_PANE_ID`).
    /// If an agent is running there, it is the one asking, and an agent may
    /// stop only itself and the agents it spawned. The owner - the office, or
    /// a pane with no agent in it - may stop anyone.
    #[serde(default)]
    pub from_pane: Option<String>,
    /// Which workspace the label means, for a caller with no pane of its own.
    /// Labels are unique per workspace, so `reviewer` may name an agent in
    /// each of them; ids need no workspace.
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Args for `agent.prompt`: type the owner's turn into an agent's terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptAgentArgs {
    /// Agent id or label, or the label or id of the pane it runs in.
    pub agent: String,
    /// The prompt. Flattened to one line: in a terminal a newline is Enter.
    pub text: String,
    /// The pane the request came from, when it came from one (`DEX_PANE_ID`):
    /// its workspace is where a label is looked for.
    #[serde(default)]
    pub from_pane: Option<String>,
    /// Which workspace the label means, for a caller with no pane of its own.
    /// Labels are unique per workspace, so `reviewer` may name an agent in
    /// each of them; ids need no workspace.
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Result of `agent.prompt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompted {
    /// The agent prompted.
    pub agent: String,
    /// The pane it was typed into.
    pub pane: String,
}

/// Result of `agent.stop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stopped {
    /// The agent asked to stop.
    pub agent: String,
    /// The pane Ctrl+C was pressed in; `None` when the agent's pane was
    /// already gone and stopping meant only marking it ended.
    pub pane: Option<String>,
    /// Whether that pane was closed as well: true for an agent Dex spawned,
    /// whose pane Dex made; false for one the owner started in a pane of theirs.
    #[serde(default)]
    pub closed_pane: bool,
}

/// Args for `agent.pane_exited`: a pane's process exited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneExitedArgs {
    /// The pane.
    pub pane: String,
}
