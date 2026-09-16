//! `dex-mcp`: a stdio MCP server exposing the workspace's shared context to the
//! Claude Code session running in a Dex pane (docs/prd.md §10.2).
//!
//! **No pane, no tools.** The registration is user-scope, so this binary starts
//! in *every* Claude Code session on the machine, most of which have nothing to
//! do with Dex. When `DEX_PANE_ID` is absent it advertises an empty tool list
//! and never touches the pipe: zero context cost and zero startup cost in
//! unrelated work. It never guesses a workspace or attaches to the most recent
//! one. The gate is read once, at startup, because it cannot change afterwards.
//!
//! The JSON-RPC layer is hand-rolled rather than taken from the `rmcp` SDK. The
//! surface needed is four methods, the tool list has to be decided at runtime
//! rather than by a derive macro, and this process starts once per Claude Code
//! session, so avoiding an async runtime keeps that startup cheap.
#![forbid(unsafe_code)]

mod bridge;
mod tools;

use std::io::{BufRead, Write};

use dex_cli::client::Client;
use serde_json::{Value, json};

/// The MCP revision this server implements when the client does not name one.
const FALLBACK_PROTOCOL: &str = "2025-06-18";

/// JSON-RPC: the method does not exist.
const METHOD_NOT_FOUND: i64 = -32601;

/// One run of the server: the pane it belongs to, and the pipe once opened.
struct Session {
    /// `None` when this is not a Dex pane, which disables everything.
    pane: Option<String>,
    /// Opened on the first tool call, never before.
    client: Option<Client>,
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut session = Session {
        pane: bridge::pane(),
        client: None,
    };

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        // A notification has no id and takes no reply.
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let reply = match handle(method, &params, &mut session) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        if writeln!(stdout, "{reply}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
}

fn handle(method: &str, params: &Value, session: &mut Session) -> Result<Value, Value> {
    match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list(session.pane.is_some()) })),
        "tools/call" => Ok(call_tool(params, session)),
        _ => {
            Err(json!({ "code": METHOD_NOT_FOUND, "message": format!("unknown method {method}") }))
        }
    }
}

fn initialize(params: &Value) -> Value {
    // Echo the client's revision when it names one: this server's surface is
    // the same across the revisions in use, and disagreeing would drop the
    // connection for no benefit.
    let protocol = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(FALLBACK_PROTOCOL);
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "dex", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// The tools on offer: all of them inside a Dex pane, none outside one.
fn tool_list(in_pane: bool) -> Vec<Value> {
    if in_pane {
        tools::advertised()
    } else {
        Vec::new()
    }
}

fn call_tool(params: &Value, session: &mut Session) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    let Some(pane) = session.pane.clone() else {
        return text_error(
            "Dex is not available here: this Claude Code session is not running in a Dex pane.",
        );
    };
    let Some(tool) = tools::find(name) else {
        return text_error(&format!("There is no tool called {name}."));
    };
    let client = match connect(&mut session.client) {
        Ok(client) => client,
        Err(message) => return text_error(&message),
    };
    match bridge::call(client, &pane, tool, &arguments) {
        Ok(mut text) => {
            // Every result carries whatever the other agents have done since
            // this one last looked, so an agent that only ever calls tools
            // still stays current (PRD §10.2).
            if let Some(delta) = bridge::pending_delta(client, &pane) {
                text.push_str("\n\n");
                text.push_str(&delta);
            }
            json!({ "content": [{ "type": "text", "text": text }] })
        }
        Err(err) => bridge::tool_failure(&err),
    }
}

/// Opens the pipe on first use and keeps it for the session.
fn connect(client: &mut Option<Client>) -> Result<&mut Client, String> {
    if client.is_none() {
        match dex_cli::client::connect() {
            Ok(fresh) => *client = Some(fresh),
            Err(err) => return Err(format!("{}\n\n{}", err.message, err.repair)),
        }
    }
    Ok(client.as_mut().expect("just connected"))
}

/// A tool failure the model can read. MCP reports these in the result rather
/// than as a JSON-RPC error, so the model can react instead of the call
/// failing outright.
fn text_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outside() -> Session {
        Session {
            pane: None,
            client: None,
        }
    }

    #[test]
    fn outside_a_dex_pane_there_are_no_tools_at_all() {
        assert!(
            tool_list(false).is_empty(),
            "a user-scope server must cost nothing in unrelated sessions"
        );
    }

    #[test]
    fn inside_a_pane_every_tool_is_offered() {
        let listed = tool_list(true);
        assert_eq!(listed.len(), 8);
        assert!(listed.iter().all(|tool| tool["inputSchema"].is_object()));
        assert!(listed.iter().all(|tool| tool["description"].is_string()));
    }

    #[test]
    fn calling_a_tool_outside_a_pane_explains_itself_without_touching_the_pipe() {
        let mut session = outside();
        let result = call_tool(&json!({ "name": "note_append" }), &mut session);
        assert_eq!(result["isError"], true);
        assert!(session.client.is_none(), "it must not open the pipe");
    }

    #[test]
    fn tools_list_is_empty_through_the_dispatcher_too() {
        let mut session = outside();
        let reply = handle("tools/list", &Value::Null, &mut session).unwrap();
        assert_eq!(reply["tools"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn initialize_agrees_with_the_client_on_the_revision() {
        let reply = initialize(&json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(reply["protocolVersion"], "2025-03-26");
        assert_eq!(reply["serverInfo"]["name"], "dex");
        assert_eq!(
            initialize(&Value::Null)["protocolVersion"],
            FALLBACK_PROTOCOL
        );
    }

    #[test]
    fn an_unknown_method_is_a_json_rpc_error_not_a_crash() {
        let mut session = outside();
        let error = handle("nope", &Value::Null, &mut session).unwrap_err();
        assert_eq!(error["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn ping_is_answered() {
        let mut session = outside();
        assert_eq!(
            handle("ping", &Value::Null, &mut session).unwrap(),
            json!({})
        );
    }
}
