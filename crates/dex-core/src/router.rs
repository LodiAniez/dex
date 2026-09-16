//! Maps `cmd` strings to slice handlers.
//!
//! The only module that knows every slice exists (docs/conventions.md §1.2).
//! One line per command. It is also the single place where every error gets
//! its protocol code and repair string (§4.3), so no error can reach a client
//! without an actionable `repair`, and where changes are announced to the UI.

mod repairs;

use dex_protocol::{Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use crate::app::AppState;
use crate::features::agent::{self, AgentError};
use crate::features::context::{self, ContextError};
use crate::features::diagnostics::{self, DiagnosticsError};
use crate::features::repo::{self, RepoError};
use crate::features::workspace::{self, WorkspaceError};
use repairs::error_body;

/// Which state a successful command changed, so the UI re-reads it; `None`
/// for commands that change nothing the UI shows. The watchdog announces its
/// own changes, since most sweeps change nothing.
fn changes(cmd: &str) -> Option<&'static str> {
    match cmd {
        "workspace.list" | "pane.list" | "pane.send" | "pane.send_key" | "pane.content" => None,
        "agent.list" | "agent.sweep" => None,
        "context.read" | "context.list" | "context.search" | "context.events" => None,
        // A digest advances the caller's cursor, which no client displays.
        "context.digest" => None,
        "repo.list" | "repo.status" | "repo.diff" | "worktree.list" => None,
        // `config.reload` may change what is in force, and the UI reads
        // keybindings from it. `config.get` changes nothing.
        "config.get" => None,
        "config.reload" => Some("config"),
        repo if repo.starts_with("repo.") || repo.starts_with("worktree.") => Some("repos"),
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
        "pane.content" => encode(workspace::pane_content(state, args(req)?).await?),
        "repo.diff" => encode(repo::diff(state, args(req)?).await?),
        "repo.add" => encode(repo::add(state, args(req)?).await?),
        "repo.list" => encode(repo::list(state).await?),
        "repo.scan" => encode(repo::scan(state, args(req)?).await?),
        "repo.status" => encode(repo::status(state, args(req)?).await?),
        "worktree.add" => encode(repo::add_worktree(state, args(req)?).await?),
        "worktree.remove" => encode(repo::remove_worktree(state, args(req)?).await?),
        "worktree.list" => encode(repo::list_worktrees(state, args(req)?).await?),
        "context.read" => encode(context::read(state, args(req)?).await?),
        "context.write" => encode(context::write(state, args(req)?).await?),
        "context.list" => encode(context::list(state, args(req)?).await?),
        "context.search" => encode(context::search(state, args(req)?).await?),
        "context.note" => encode(context::note(state, args(req)?).await?),
        "context.message_send" => encode(context::message_send(state, args(req)?).await?),
        "context.inbox" => encode(context::inbox(state, args(req)?).await?),
        "context.events" => encode(context::events(state, args(req)?).await?),
        "context.delete_event" => encode(context::delete_event(state, args(req)?).await?),
        "context.clear_events" => encode(context::clear_events(state, args(req)?).await?),
        "context.digest" => encode(context::digest(state, args(req)?).await?),
        "agent.event" => encode(agent::event(state, args(req)?).await?),
        "agent.list" => encode(agent::list(state, args(req)?).await?),
        "agent.stop" => encode(agent::stop(state, args(req)?).await?),
        "agent.spawn" => encode(agent::spawn(state, args(req)?).await?),
        "agent.pane_exited" => encode(agent::pane_exited(state, args(req)?).await?),
        "agent.sweep" => encode(agent::sweep(state).await?),
        "config.get" => encode(diagnostics::get_config(state)?),
        "config.reload" => encode(diagnostics::reload_config(state)?),
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
    #[error(transparent)]
    Repo(#[from] RepoError),
    #[error(transparent)]
    Diagnostics(#[from] DiagnosticsError),
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

#[cfg(test)]
mod tests {
    use dex_protocol::ErrorCode;
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
