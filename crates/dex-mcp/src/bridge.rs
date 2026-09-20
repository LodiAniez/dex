//! Turning one tool call into one daemon request, and back into text for the
//! model (docs/prd.md §10.2). This server holds no state of its own.

use dex_cli::client::Client;
use dex_protocol::ErrorBody;
use dex_protocol::context::Digest;
use serde_json::{Map, Value, json};

use crate::tools::Tool;

/// Who is calling, from the environment Claude Code passed down.
///
/// `DEX_AGENT_ID` identifies an agent Dex spawned; otherwise the daemon uses
/// whichever agent is live in this pane, so a human-started `claude` still
/// writes under its own name.
pub fn caller(pane: &str) -> Value {
    let mut caller = Map::new();
    caller.insert("pane".into(), pane.into());
    if let Some(agent) = usable(var("DEX_AGENT_ID").as_deref()) {
        caller.insert("agent".into(), agent.into());
    }
    Value::Object(caller)
}

/// The pane this server was started in, read once: it cannot change while the
/// process runs, and the whole tool list hangs off it.
pub fn pane() -> Option<String> {
    usable(var("DEX_PANE_ID").as_deref())
}

/// A `DEX_*` value Dex actually set, or `None`.
///
/// Absent and empty both mean "not in a Dex pane". So does a value containing
/// `${`: the user-scope registration passes `${DEX_PANE_ID:-}`, and a Claude
/// Code version that does not expand it hands the literal text straight through
/// (PRD §10.2).
pub fn usable(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.is_empty() && !value.contains("${"))
        .map(str::to_owned)
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Runs one tool call and renders the result as the text the model reads.
pub fn call(
    client: &mut Client,
    pane: &str,
    tool: &Tool,
    arguments: &Value,
) -> Result<String, ErrorBody> {
    let mut args = match arguments {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    // `agents_list` takes no arguments of its own but must still be scoped:
    // agents in another workspace are none of this agent's business.
    if tool.name == "agents_list" {
        args.insert("include_dead".into(), false.into());
    }
    for (key, value) in caller(pane).as_object().into_iter().flatten() {
        args.insert(key.clone(), value.clone());
    }
    let data: Value = client.call(tool.command, Value::Object(args))?;
    Ok(render(tool.name, &data))
}

/// What the model sees for a successful call.
fn render(name: &str, data: &Value) -> String {
    match name {
        // A write is a confirmation, not a document: returning the whole entry
        // back to the model would just spend tokens repeating its own input.
        "context_write" => format!(
            "Stored. It is now at version {}. Pass that as expected_version next time.",
            data.get("version").and_then(Value::as_i64).unwrap_or(0)
        ),
        "note_append" => "Recorded. Other agents will see it at their next turn.".to_owned(),
        // Whether it was woken matters to the sender: an agent that is
        // working reads the message at its next turn, not now.
        "message_send" => match data.get("delivery").and_then(Value::as_str) {
            Some("woken") => "Sent. They had finished their turn, and have been woken to read it.",
            Some("waiting_on_owner") => {
                "Sent. They are waiting on the owner; they will read it when the owner answers."
            }
            Some("ended") => "Sent, but that agent has ended: nothing will read it.",
            _ => "Sent. They are working; they will see it at their next turn.",
        }
        .to_owned(),
        "message_inbox" => match data.get("messages").and_then(Value::as_array) {
            Some(messages) if messages.is_empty() => "No messages.".to_owned(),
            _ => pretty(data),
        },
        _ => pretty(data),
    }
}

fn pretty(data: &Value) -> String {
    serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
}

/// What other agents have done since this one last looked, appended to every
/// tool result.
///
/// This is the pull half of delta delivery: it needs no hook support, advances
/// the same cursor the hooks use so nothing is shown twice, and is the fallback
/// if hook behaviour shifts (PRD §10.2).
pub fn pending_delta(client: &mut Client, pane: &str) -> Option<String> {
    let mut args = match caller(pane) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    args.insert("kind".into(), "delta".into());
    args.insert("rate_limited".into(), false.into());
    client
        .call::<Digest>("context.digest", Value::Object(args))
        .ok()
        .and_then(|digest| digest.text)
}

/// The JSON-RPC error body for a failed tool call, carrying the daemon's
/// repair string: an agent that reads it can often fix the call itself.
pub fn tool_failure(err: &ErrorBody) -> Value {
    json!({
        "content": [{ "type": "text", "text": format!("{}\n\n{}", err.message, err.repair) }],
        "isError": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unexpanded_placeholder_is_not_a_pane() {
        // What the user-scope registration passes when Claude Code does not
        // expand it, and what it passes outside a Dex pane.
        assert_eq!(usable(Some("${DEX_PANE_ID:-}")), None);
        assert_eq!(usable(Some("${DEX_PANE_ID}")), None);
        assert_eq!(usable(Some("")), None);
        assert_eq!(usable(None), None);
        assert_eq!(usable(Some("a1b2c3")).as_deref(), Some("a1b2c3"));
    }

    #[test]
    fn a_write_result_tells_the_model_what_to_do_next() {
        let text = render("context_write", &json!({ "version": 4 }));
        assert!(text.contains("version 4"), "{text}");
        assert!(text.contains("expected_version"), "{text}");
    }

    #[test]
    fn an_empty_inbox_reads_as_a_sentence_not_as_json() {
        assert_eq!(
            render("message_inbox", &json!({ "messages": [] })),
            "No messages."
        );
    }

    #[test]
    fn a_failure_carries_the_repair() {
        let body = tool_failure(&ErrorBody {
            code: dex_protocol::ErrorCode::InvalidArgs,
            message: "bad key".into(),
            repair: "Use lowercase.".into(),
        });
        assert_eq!(body["isError"], true);
        let text = body["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("bad key") && text.contains("Use lowercase."));
    }
}
