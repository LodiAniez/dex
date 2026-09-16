//! `dex event <kind>`: the Claude Code hook entry point (docs/prd.md §9.3).
//!
//! Always exits 0 and prints nothing on stdout: a failing or noisy hook
//! interrupts the agent's actual work. A problem becomes one `Warning:` line
//! on stderr, unless `DEX_SILENT=1`. (From M6, digest hooks print their digest
//! on stdout here.)

use std::io::{IsTerminal, Read};
use std::time::{SystemTime, UNIX_EPOCH};

use dex_protocol::agent::EventOutcome;
use dex_protocol::context::Digest;
use serde_json::{Value, json};

use dex_cli::client;

/// Hook input is small; anything past this is not a hook payload.
const MAX_INPUT_BYTES: u64 = 1 << 20;

/// Forwards one hook to the daemon. `main` has already returned if this is not
/// running inside a Dex pane.
pub fn run(kind: &str) {
    // First, before any I/O: status events are ordered by when the hook
    // started, because background hooks reach the daemon in any order.
    let stamp = now_millis();
    let pane = std::env::var("DEX_PANE_ID").unwrap_or_default();
    let agent = std::env::var("DEX_AGENT_ID")
        .ok()
        .filter(|id| !id.is_empty());
    let input = read_input();
    let args =
        json!({ "kind": kind, "pane": pane, "agent": agent, "stamp": stamp, "input": &input });
    let mut dex = match client::connect() {
        Ok(dex) => dex,
        Err(err) => return warn(&err.message),
    };
    if let Err(err) = dex.call::<EventOutcome>("agent.event", args) {
        warn(&err.message);
    }
    // Hooks that Claude Code waits for can hand the model text. The rest are
    // async side effects and anything they print is discarded.
    if let Some(event_name) = digest_event(kind) {
        // Hooks fire inside Claude Code's own subagents too, carrying the
        // parent's session and pane. Injecting there would show one agent's
        // context to another and trigger a delta per subagent (PRD §9.1).
        if !input.get("agent_id").is_some_and(|id| !id.is_null()) {
            emit_digest(&mut dex, kind, event_name, &pane, agent.as_deref());
        }
    }
}

/// The Claude Code event a `dex event <kind>` corresponds to, for the hooks
/// that carry a digest. `None` for every other kind, which prints nothing.
fn digest_event(kind: &str) -> Option<&'static str> {
    match kind {
        "session-start" => Some("SessionStart"),
        "prompt" => Some("UserPromptSubmit"),
        "batch" => Some("PostToolBatch"),
        _ => None,
    }
}

/// Asks the daemon what this agent should be told, and prints it as the JSON
/// Claude Code expects.
///
/// Prints **nothing at all** when there is no digest — not an empty object and
/// not "no updates". This runs on every prompt of every agent, so anything
/// printed here is paid for over and over (PRD §10.3).
fn emit_digest(
    dex: &mut client::Client,
    kind: &str,
    event_name: &str,
    pane: &str,
    agent: Option<&str>,
) {
    let args = json!({
        "kind": if kind == "session-start" { "full" } else { "delta" },
        // Only PostToolBatch is rate limited: it runs between model turns, so
        // an autonomous agent would otherwise get a delta every few seconds.
        "rate_limited": kind == "batch",
        "pane": pane,
        "agent": agent,
    });
    match dex.call::<Digest>("context.digest", args) {
        Ok(Digest { text: Some(text) }) => println!(
            "{}",
            json!({
                "hookSpecificOutput": {
                    "hookEventName": event_name,
                    "additionalContext": text,
                }
            })
        ),
        Ok(Digest { text: None }) => {}
        Err(err) => warn(&err.message),
    }
}

/// The hook's stdin JSON, or null.
fn read_input() -> Value {
    let stdin = std::io::stdin();
    // Run by hand in a terminal there is no hook JSON, and reading would wait forever.
    if stdin.is_terminal() {
        return Value::Null;
    }
    let mut text = String::new();
    if stdin
        .lock()
        .take(MAX_INPUT_BYTES)
        .read_to_string(&mut text)
        .is_err()
    {
        return Value::Null;
    }
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

fn warn(message: &str) {
    if std::env::var_os("DEX_SILENT").is_some_and(|value| value == "1") {
        return;
    }
    eprintln!("Warning: dex event: {message}");
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
