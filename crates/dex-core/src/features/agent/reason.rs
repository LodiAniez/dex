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
                Some(("command", command)) => format!("permission to {tool}: {}", gist(&command)),
                Some(("file_path" | "path", path)) => {
                    format!("permission to {tool} {}", tail_of(&path))
                }
                Some((_, target)) => format!("permission to {tool} {target}"),
                None => format!("permission to {tool}"),
            }
        }
        None => text(input, "message")?,
    };
    Some(one_short_line(&reason))
}

/// The end of a path: the folder and the file. Claude Code sends absolute paths
/// (captured from 2.1.274), and cut to length from the front they lose the one
/// part that says which file.
fn tail_of(path: &str) -> String {
    let parts: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() <= 3 {
        return path.replace('\\', "/");
    }
    format!("…/{}", parts[parts.len() - 2..].join("/"))
}

/// The most words of a command worth showing: the program and what it is asked to do.
const GIST_WORDS: usize = 3;

/// What a command runs, without what it carries. The reason is stored, shown in
/// the office, and written to the activity log; a command line is where tokens,
/// passwords and signed URLs live. So: the first few words, stopping before
/// anything that could hold a value - an assignment, a quoted string, a
/// credential-bearing URL, a flag with its value run into it (`-psecret`), or
/// anything long and opaque. Said plainly when something was left out.
fn gist(command: &str) -> String {
    let carries_a_value = |word: &str| {
        word.contains('=')
            || word.contains('@')
            || word.contains("://")
            || word.contains(['\'', '"', '`', '$'])
            || (word.starts_with('-') && !word.starts_with("--") && word.chars().count() > 2)
            || word.chars().count() > 32
    };
    let words: Vec<&str> = command.split_whitespace().collect();
    let kept: Vec<&str> = words
        .iter()
        .take(GIST_WORDS)
        .take_while(|word| !carries_a_value(word))
        .copied()
        .collect();
    match (kept.is_empty(), kept.len() < words.len()) {
        (true, _) => "…".to_owned(),
        (false, true) => format!("{} …", kept.join(" ")),
        (false, false) => kept.join(" "),
    }
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
