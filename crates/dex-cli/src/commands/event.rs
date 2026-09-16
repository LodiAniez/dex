//! `dex event <kind>`: the Claude Code hook entry point (docs/prd.md §9.3).
//!
//! Always exits 0 and prints nothing on stdout: a failing or noisy hook
//! interrupts the agent's actual work. A problem becomes one `Warning:` line
//! on stderr, unless `DEX_SILENT=1`. (From M6, digest hooks print their digest
//! on stdout here.)

use std::io::{IsTerminal, Read};
use std::time::{SystemTime, UNIX_EPOCH};

use dex_protocol::agent::EventOutcome;
use serde_json::{Value, json};

use crate::client;

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
        json!({ "kind": kind, "pane": pane, "agent": agent, "stamp": stamp, "input": input });
    if let Err(err) =
        client::connect().and_then(|mut dex| dex.call::<EventOutcome>("agent.event", args))
    {
        warn(&err.message);
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
