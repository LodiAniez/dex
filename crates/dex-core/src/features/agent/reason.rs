//! What an agent is waiting for, read out of the hook that said it was waiting.
//!
//! "Needs you" with nothing said about what for sends the owner to the pane to
//! find out. The hook already knows: a `PermissionRequest` carries the tool and
//! its input, a `Notification` carries a message. Read tolerantly - the field
//! names are Claude Code's, and an input that says nothing useful gives no
//! reason rather than a wrong one.

use serde_json::Value;

/// The longest reason worth showing: it sits on one line of a narrow panel.
const REASON_CHARS: usize = 120;

/// The inputs worth naming, in the order they are looked for: what a tool is
/// aimed at says more than which tool it is.
const TARGETS: [&str; 5] = ["file_path", "path", "command", "url", "pattern"];

/// Why the agent is waiting, as one short line, or `None` when the hook does
/// not say.
pub fn waiting_reason(input: &Value) -> Option<String> {
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    let reason = match text(input, "tool_name") {
        Some(tool) => {
            let target = input.get("tool_input").and_then(|args| {
                TARGETS
                    .iter()
                    .find_map(|key| text(args, key).map(|found| (*key, found)))
            });
            match target {
                // A command reads better after a colon than as an object of the verb.
                Some(("command", command)) => format!("permission to {tool}: {command}"),
                Some((_, target)) => format!("permission to {tool} {target}"),
                None => format!("permission to {tool}"),
            }
        }
        None => text(input, "message")?,
    };
    Some(one_short_line(&reason))
}

fn one_short_line(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= REASON_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(REASON_CHARS - 1).collect();
    cut.push('…');
    cut
}
