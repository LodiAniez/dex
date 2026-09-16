//! Maps `cmd` strings to slice handlers.
//!
//! The only module that knows every slice exists (docs/conventions.md §1.2).
//! One line per command. It is also the single place where every error gets
//! its protocol code and repair string (§4.3), so no error can reach a client
//! without an actionable `repair`, and where changes are announced to the UI.

use dex_protocol::{ErrorBody, ErrorCode, Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use crate::app::AppState;
use crate::features::agent::{self, AgentError};
use crate::features::context::{self, ContextError};
use crate::features::workspace::{self, WorkspaceError};

/// Which state a successful command changed, so the UI re-reads it; `None`
/// for commands that change nothing the UI shows. The watchdog announces its
/// own changes, since most sweeps change nothing.
fn changes(cmd: &str) -> Option<&'static str> {
    match cmd {
        "workspace.list" | "pane.list" | "pane.send" | "pane.send_key" => None,
        "agent.list" | "agent.sweep" => None,
        "context.read" | "context.list" | "context.search" | "context.events" => None,
        // A digest advances the caller's cursor, which no client displays.
        "context.digest" => None,
        agent if agent.starts_with("agent.") => Some("agents"),
        // `context.inbox` marks messages read, which the activity pane shows.
        context if context.starts_with("context.") => Some("context"),
        _ => Some("workspaces"),
    }
}

/// Runs one request and wraps the outcome in a response. Never fails: every
/// error becomes an error response.
pub async fn dispatch(state: &AppState, request: Request) -> Response {
    let id = request.id.clone();
    match route(state, &request).await {
        Ok(data) => {
            if let Some(topic) = changes(&request.cmd) {
                state.bus.publish(topic);
            }
            Response::success(id, data)
        }
        Err(err) => Response::failure(id, error_body(&err)),
    }
}

async fn route(state: &AppState, req: &Request) -> Result<Value, CoreError> {
    match req.cmd.as_str() {
        "workspace.list" => encode(workspace::list(state).await?),
        "workspace.create" => encode(workspace::create(state, args(req)?).await?),
        "workspace.rename" => encode(workspace::rename(state, args(req)?).await?),
        "workspace.recolor" => encode(workspace::recolor(state, args(req)?).await?),
        "workspace.reorder" => encode(workspace::reorder(state, args(req)?).await?),
        "workspace.switch" => encode(workspace::switch(state, args(req)?).await?),
        "workspace.delete" => encode(workspace::delete(state, args(req)?).await?),
        "workspace.set_layout" => encode(workspace::set_layout(state, args(req)?).await?),
        "workspace.cycle_layout" => encode(workspace::cycle_layout(state, args(req)?).await?),
        "pane.list" => encode(workspace::list_panes(state, args(req)?).await?),
        "pane.create" => encode(workspace::create_pane(state, args(req)?).await?),
        "pane.split" => encode(workspace::split_pane(state, args(req)?).await?),
        "pane.close" => encode(workspace::close_pane(state, args(req)?).await?),
        "pane.focus" => encode(workspace::focus_pane(state, args(req)?).await?),
        "pane.swap" => encode(workspace::swap_panes(state, args(req)?).await?),
        "pane.label" => encode(workspace::label_pane(state, args(req)?).await?),
        "pane.send" => encode(workspace::send(state, args(req)?).await?),
        "pane.send_key" => encode(workspace::send_key(state, args(req)?).await?),
        "context.read" => encode(context::read(state, args(req)?).await?),
        "context.write" => encode(context::write(state, args(req)?).await?),
        "context.list" => encode(context::list(state, args(req)?).await?),
        "context.search" => encode(context::search(state, args(req)?).await?),
        "context.note" => encode(context::note(state, args(req)?).await?),
        "context.message_send" => encode(context::message_send(state, args(req)?).await?),
        "context.inbox" => encode(context::inbox(state, args(req)?).await?),
        "context.events" => encode(context::events(state, args(req)?).await?),
        "context.digest" => encode(context::digest(state, args(req)?).await?),
        "agent.event" => encode(agent::event(state, args(req)?).await?),
        "agent.list" => encode(agent::list(state, args(req)?).await?),
        "agent.stop" => encode(agent::stop(state, args(req)?).await?),
        "agent.pane_exited" => encode(agent::pane_exited(state, args(req)?).await?),
        "agent.sweep" => encode(agent::sweep(state).await?),
        _ => Err(CoreError::UnknownCommand(req.cmd.clone())),
    }
}

/// Every error a command can produce, aggregated from the slices.
#[derive(Debug, Error)]
enum CoreError {
    #[error("unknown command {0:?}")]
    UnknownCommand(String),
    #[error("invalid arguments for {cmd}: {reason}")]
    InvalidArgs { cmd: String, reason: String },
    #[error("could not encode the result: {0}")]
    Encode(serde_json::Error),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Context(#[from] ContextError),
}

fn args<T: DeserializeOwned>(req: &Request) -> Result<T, CoreError> {
    serde_json::from_value(req.args.clone()).map_err(|err| CoreError::InvalidArgs {
        cmd: req.cmd.clone(),
        reason: err.to_string(),
    })
}

fn encode(value: impl Serialize) -> Result<Value, CoreError> {
    serde_json::to_value(value).map_err(CoreError::Encode)
}

const REPAIR_BUG: &str =
    "This is a bug in Dex, not in your input. The app log has the details; please report it.";

/// The protocol code and repair string for every error.
fn error_body(err: &CoreError) -> ErrorBody {
    let (code, repair) = match err {
        CoreError::UnknownCommand(_) => (
            ErrorCode::InvalidArgs,
            "Check the command name; `dex --help` lists every command.".to_owned(),
        ),
        CoreError::InvalidArgs { .. } => (
            ErrorCode::InvalidArgs,
            "Check the arguments; `dex <command> --help` lists what the command accepts."
                .to_owned(),
        ),
        CoreError::Encode(_) => (ErrorCode::Internal, REPAIR_BUG.to_owned()),
        CoreError::Workspace(err) => workspace_repair(err),
        CoreError::Agent(AgentError::NoSuchPane(_)) => (
            ErrorCode::NoSuchPane,
            "The hook ran in a pane Dex no longer has; nothing to do.".to_owned(),
        ),
        CoreError::Agent(AgentError::NoSuchAgent(_)) => (
            ErrorCode::NoSuchAgent,
            "Run `dex agent list` to see the running agents; target one by its id or its pane's label."
                .to_owned(),
        ),
        CoreError::Agent(AgentError::NotRunning(_)) => (
            ErrorCode::NoSuchAgent,
            "That agent has already ended; `dex agent list` shows the running ones.".to_owned(),
        ),
        CoreError::Agent(AgentError::Target(err)) => workspace_repair(err),
        CoreError::Agent(AgentError::Db(_)) => (ErrorCode::Internal, REPAIR_BUG.to_owned()),
        CoreError::Context(err) => context_repair(err),
    };
    ErrorBody {
        code,
        message: err.to_string(),
        repair,
    }
}

fn context_repair(err: &ContextError) -> (ErrorCode, String) {
    let (code, repair) = match err {
        ContextError::InvalidKey { reason, .. } => {
            (ErrorCode::InvalidArgs, reason.repair().to_owned())
        }
        ContextError::NoSuchKey(_) => (
            ErrorCode::InvalidArgs,
            "Run `dex context list` to see the keys that exist, or write it first.".to_owned(),
        ),
        // The repair carries the current value so the caller can merge in one
        // step instead of reading, diffing, and racing again.
        ContextError::VersionConflict { current, value, .. } => {
            return (
                ErrorCode::VersionConflict,
                format!(
                    "Someone else wrote this key first. It now holds {value:?} at version \
                     {current}. Merge your change into that value and write again with \
                     expected_version={current}."
                ),
            );
        }
        ContextError::NoSuchAgent(_) => (
            ErrorCode::NoSuchAgent,
            "Run `dex agent list` to see which agents are running, and target one by label."
                .to_owned(),
        ),
        ContextError::NoWorkspace => (
            ErrorCode::NotInPane,
            "Run this inside a Dex pane, or pass --workspace <name-or-id>.".to_owned(),
        ),
        ContextError::Target(err) => return workspace_repair(err),
        ContextError::Db(_) => (ErrorCode::Internal, REPAIR_BUG.to_owned()),
    };
    (code, repair)
}

fn workspace_repair(err: &WorkspaceError) -> (ErrorCode, String) {
    let (code, repair) = match err {
        WorkspaceError::NoSuchWorkspace(_) => (
            ErrorCode::NoSuchWorkspace,
            "Run `dex workspace list` to see the workspaces that exist.",
        ),
        WorkspaceError::NoSuchPane(_) => (
            ErrorCode::NoSuchPane,
            "Run `dex pane list` to see the panes that exist.",
        ),
        WorkspaceError::AmbiguousTarget { candidates, .. } => {
            return (
                ErrorCode::AmbiguousTarget,
                format!(
                    "Target one of them by id instead: {}.",
                    candidates.join("; ")
                ),
            );
        }
        WorkspaceError::PaneNotStarted(_) => (
            ErrorCode::NoSuchPane,
            "A pane's shell starts when the pane is first shown: switch to its workspace in Dex, then retry.",
        ),
        WorkspaceError::InvalidColor(_) => (
            ErrorCode::InvalidArgs,
            "Use a hex color like #4f8cff, or no color.",
        ),
        WorkspaceError::InvalidName => (
            ErrorCode::InvalidArgs,
            "Give the workspace a name between 1 and 64 characters.",
        ),
        WorkspaceError::InvalidOrder => (
            ErrorCode::InvalidArgs,
            "List every workspace id exactly once, in the new order.",
        ),
        WorkspaceError::InvalidRoot(_) => (
            ErrorCode::InvalidArgs,
            "Choose an existing folder, as an absolute path.",
        ),
        WorkspaceError::LastPane => (
            ErrorCode::InvalidArgs,
            "A workspace keeps at least one pane; delete the workspace to close it.",
        ),
        WorkspaceError::LayoutMismatch => (
            ErrorCode::InvalidArgs,
            "Send a layout that shows every pane of the workspace exactly once.",
        ),
        WorkspaceError::LabelTaken(_) => (
            ErrorCode::InvalidArgs,
            "Pick another label, or clear the other pane's label first.",
        ),
        WorkspaceError::InvalidLabel => (
            ErrorCode::InvalidArgs,
            "Use a short label without spaces, like `server` or `tests`.",
        ),
        WorkspaceError::Pty(_) | WorkspaceError::Layout(_) | WorkspaceError::Db(_) => {
            (ErrorCode::Internal, REPAIR_BUG)
        }
    };
    (code, repair.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request(cmd: &str, args: Value) -> Request {
        Request {
            id: "t1".into(),
            cmd: cmd.into(),
            args,
        }
    }

    #[tokio::test]
    async fn unknown_commands_fail_with_a_repair_string() {
        let (_dir, state) = AppState::for_tests();
        let response = dispatch(&state, request("nope.nope", json!({}))).await;
        let error = response.error.unwrap();
        assert_eq!(error.code, ErrorCode::InvalidArgs);
        assert!(!error.repair.is_empty());
        assert_eq!(response.id, "t1");
    }

    #[tokio::test]
    async fn malformed_arguments_are_invalid_args() {
        let (_dir, state) = AppState::for_tests();
        let response = dispatch(&state, request("workspace.rename", json!({"name": 5}))).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn create_then_list_round_trips_through_the_router() {
        let (_dir, state) = AppState::for_tests();
        let created = dispatch(&state, request("workspace.create", json!({"name": "api"}))).await;
        assert!(created.ok, "{:?}", created.error);
        let listed = dispatch(&state, request("workspace.list", json!({}))).await;
        assert_eq!(listed.data.unwrap()["workspaces"][0]["name"], "api");
    }

    #[tokio::test]
    async fn every_error_carries_a_non_empty_repair() {
        let (_dir, state) = AppState::for_tests();
        let missing = dispatch(
            &state,
            request("workspace.switch", json!({"workspace": "gone"})),
        )
        .await;
        let error = missing.error.unwrap();
        assert_eq!(error.code, ErrorCode::NoSuchWorkspace);
        assert!(error.repair.contains("dex workspace list"));
    }

    #[tokio::test]
    async fn pane_commands_are_routed() {
        let (_dir, state) = AppState::for_tests();
        let created = dispatch(&state, request("workspace.create", json!({}))).await;
        let pane = created.data.unwrap()["workspaces"][0]["panes"][0]["id"].clone();

        let split = dispatch(&state, request("pane.split", json!({"pane": pane}))).await;
        assert!(split.ok, "{:?}", split.error);
        let panes = dispatch(&state, request("pane.list", json!({}))).await;
        assert_eq!(
            panes.data.unwrap()["panes"].as_array().map(Vec::len),
            Some(2)
        );

        let missing = dispatch(&state, request("pane.close", json!({"pane": "gone"}))).await;
        assert_eq!(missing.error.unwrap().code, ErrorCode::NoSuchPane);
    }

    #[tokio::test]
    async fn changes_are_announced_to_the_ui_and_reads_are_not() {
        let (_dir, state) = AppState::for_tests();
        let mut changes = state.bus.subscribe();

        dispatch(&state, request("workspace.create", json!({}))).await;
        assert_eq!(changes.try_recv().unwrap().topic, "workspaces");

        dispatch(&state, request("workspace.list", json!({}))).await;
        assert!(
            changes.try_recv().is_err(),
            "a read must not announce a change"
        );
    }

    #[tokio::test]
    async fn ambiguous_targets_list_the_candidates_in_the_repair() {
        let (_dir, state) = AppState::for_tests();
        dispatch(&state, request("workspace.create", json!({"name": "dup"}))).await;
        dispatch(&state, request("workspace.create", json!({"name": "dup"}))).await;
        let response = dispatch(
            &state,
            request("workspace.switch", json!({"workspace": "dup"})),
        )
        .await;
        let error = response.error.unwrap();
        assert_eq!(error.code, ErrorCode::AmbiguousTarget);
        assert_eq!(error.repair.matches("dup (").count(), 2);
    }
}
