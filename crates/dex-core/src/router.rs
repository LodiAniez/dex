//! Maps `cmd` strings to slice handlers.
//!
//! The only module that knows every slice exists (docs/conventions.md §1.2).
//! One line per command. It is also the single place where every error gets
//! its protocol code and repair string (§4.3), so no error can reach a client
//! without an actionable `repair`.

use dex_protocol::{ErrorBody, ErrorCode, Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use crate::app::AppState;
use crate::features::workspace::{self, WorkspaceError};

/// Runs one request and wraps the outcome in a response. Never fails: every
/// error becomes an error response.
pub async fn dispatch(state: &AppState, request: Request) -> Response {
    let id = request.id.clone();
    match route(state, &request).await {
        Ok(data) => Response::success(id, data),
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
        "pane.split" => encode(workspace::split_pane(state, args(req)?).await?),
        "pane.close" => encode(workspace::close_pane(state, args(req)?).await?),
        "pane.focus" => encode(workspace::focus_pane(state, args(req)?).await?),
        "pane.swap" => encode(workspace::swap_panes(state, args(req)?).await?),
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
            "Check the command name; `dex --help` lists every command.",
        ),
        CoreError::InvalidArgs { .. } => (
            ErrorCode::InvalidArgs,
            "Check the arguments; `dex <command> --help` lists what the command accepts.",
        ),
        CoreError::Encode(_) => (ErrorCode::Internal, REPAIR_BUG),
        CoreError::Workspace(err) => workspace_code(err),
    };
    ErrorBody {
        code,
        message: err.to_string(),
        repair: repair.to_owned(),
    }
}

fn workspace_code(err: &WorkspaceError) -> (ErrorCode, &'static str) {
    match err {
        WorkspaceError::NoSuchWorkspace(_) => (
            ErrorCode::NoSuchWorkspace,
            "Run `dex workspace list` to see the workspaces that exist.",
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
            "Choose an existing folder as the workspace root.",
        ),
        WorkspaceError::NoSuchPane(_) => (
            ErrorCode::NoSuchPane,
            "Run `dex pane list` to see the panes that exist.",
        ),
        WorkspaceError::LastPane => (
            ErrorCode::InvalidArgs,
            "A workspace keeps at least one pane; delete the workspace to close it.",
        ),
        WorkspaceError::LayoutMismatch => (
            ErrorCode::InvalidArgs,
            "Send a layout that shows every pane of the workspace exactly once.",
        ),
        WorkspaceError::Layout(_) | WorkspaceError::Db(_) => (ErrorCode::Internal, REPAIR_BUG),
    }
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
        assert_eq!(
            split.data.unwrap()["workspaces"][0]["panes"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );

        let missing = dispatch(&state, request("pane.close", json!({"pane": "gone"}))).await;
        assert_eq!(missing.error.unwrap().code, ErrorCode::NoSuchPane);
    }
}
