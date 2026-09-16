//! Building the text injected into agents by hooks (docs/prd.md §10.3). Pure:
//! callers gather the rows, this decides what an agent is told and in how many
//! characters.
//!
//! **Write factual statements, never instructions.** This text is injected into
//! another model's context out of band. Anything phrased as a command ("now do
//! X") reads like a prompt-injection attempt, and Claude Code may surface it to
//! the user instead of using it. Describe what happened; let the agent decide.
//!
//! Times are relative ("15 minutes ago"). Injected text is saved in the
//! transcript and replayed on `--resume`, where an absolute timestamp would be
//! a lie.

/// Hard caps from PRD §10.3, in characters.
pub const FULL_CAP: usize = 2000;
pub const DELTA_CAP: usize = 800;

/// One other agent in the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sibling {
    pub label: String,
    pub status: String,
    pub task: Option<String>,
}

/// What a full digest is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Orientation {
    pub workspace: String,
    pub task_brief: Option<String>,
    pub siblings: Vec<Sibling>,
    /// Entry keys, most recently updated first.
    pub entries: Vec<String>,
    /// How many directed messages are waiting.
    pub unread: usize,
}

/// One logged event, already resolved to display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub author: String,
    pub kind: String,
    pub key: Option<String>,
    pub body: String,
    pub created_at: i64,
}

/// The heading every digest sits under, so an agent can recognise it.
const HEADING: &str = "## Workspace context";

/// An agent's orientation when its session starts.
pub fn full(orientation: &Orientation, cap: usize) -> String {
    let mut lines = vec![format!("Workspace: {}", orientation.workspace)];
    if let Some(task) = &orientation.task_brief {
        lines.push(format!("This agent's task: {task}"));
    }
    if !orientation.siblings.is_empty() {
        lines.push("Other agents here:".into());
        for sibling in &orientation.siblings {
            let task = sibling
                .task
                .as_deref()
                .map(|t| format!(" — {t}"))
                .unwrap_or_default();
            lines.push(format!("- {} ({}){task}", sibling.label, sibling.status));
        }
    }
    if !orientation.entries.is_empty() {
        lines.push("Shared context keys, most recently updated first:".into());
        for key in &orientation.entries {
            lines.push(format!("- {key}"));
        }
    }
    if orientation.unread > 0 {
        lines.push(format!(
            "{} unread {} waiting, readable with message_inbox.",
            orientation.unread,
            plural(orientation.unread, "message", "messages"),
        ));
    }
    // Keeps the head: the workspace and task matter more than the last key.
    fit(&lines, cap, Keep::Head).unwrap_or_else(|| HEADING.to_owned())
}

/// What changed since an agent last looked, or `None` when nothing did.
///
/// Returning `None` is load-bearing: the hook must emit zero bytes rather than
/// "no updates", which would otherwise cost tokens on every prompt of every
/// agent (PRD §10.3, and a mandatory regression test).
pub fn delta(changes: &[Change], now: i64, cap: usize) -> Option<String> {
    if changes.is_empty() {
        return None;
    }
    let lines: Vec<String> = collapse(changes)
        .iter()
        .map(|change| {
            format!(
                "- {} {} ({})",
                change.author,
                change.body,
                ago(now, change.created_at)
            )
        })
        .collect();
    // Keeps the head: `changes` is newest first, so the oldest is dropped.
    fit(&lines, cap, Keep::Head)
}

/// Collapses repeated writes to one key into the latest, so a busy agent does
/// not spend the whole budget on one file being saved ten times.
fn collapse(changes: &[Change]) -> Vec<&Change> {
    let mut seen: Vec<&str> = Vec::new();
    let mut kept = Vec::new();
    for change in changes {
        match change.key.as_deref() {
            Some(key) if change.kind == "write" => {
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
            }
            _ => {}
        }
        kept.push(change);
    }
    kept
}

/// Which end of the list survives truncation.
enum Keep {
    Head,
}

/// Joins lines under the heading within `cap` characters, noting any dropped.
fn fit(lines: &[String], cap: usize, keep: Keep) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    let Keep::Head = keep;
    let mut text = HEADING.to_owned();
    let mut used = 0;
    for line in lines {
        if text.len() + 1 + line.len() > cap.saturating_sub(marker_len(lines.len())) {
            break;
        }
        text.push('\n');
        text.push_str(line);
        used += 1;
    }
    let dropped = lines.len() - used;
    if dropped > 0 {
        text.push('\n');
        text.push_str(&omission(dropped));
    }
    (used > 0 || dropped > 0).then_some(text)
}

fn omission(dropped: usize) -> String {
    format!("[… {dropped} earlier updates omitted; context_search finds them …]")
}

/// Room reserved so the omission note itself always fits.
fn marker_len(total: usize) -> usize {
    omission(total).len() + 1
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// A coarse relative time. Deliberately vague: this text outlives its moment.
pub fn ago(now: i64, then: i64) -> String {
    let seconds = (now - then).max(0) / 1000;
    match seconds {
        0..=44 => "just now".into(),
        45..=5400 => {
            let minutes = (seconds + 30) / 60;
            format!(
                "{minutes} {} ago",
                plural(minutes as usize, "minute", "minutes")
            )
        }
        _ => {
            let hours = (seconds + 1800) / 3600;
            format!("{hours} {} ago", plural(hours as usize, "hour", "hours"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(author: &str, body: &str, key: Option<&str>, at: i64) -> Change {
        Change {
            author: author.into(),
            kind: if key.is_some() { "write" } else { "note" }.into(),
            key: key.map(str::to_owned),
            body: body.into(),
            created_at: at,
        }
    }

    #[test]
    fn an_empty_delta_is_nothing_at_all() {
        assert_eq!(delta(&[], 0, DELTA_CAP), None);
    }

    #[test]
    fn a_delta_lists_what_others_did_with_relative_times() {
        let now = 1_000_000;
        let changes = [change(
            "backend",
            "noted the schema moved",
            None,
            now - 600_000,
        )];
        let text = delta(&changes, now, DELTA_CAP).unwrap();
        assert!(text.starts_with(HEADING));
        assert!(text.contains("backend noted the schema moved"));
        assert!(text.contains("10 minutes ago"), "{text}");
    }

    #[test]
    fn a_delta_never_exceeds_its_budget_and_says_what_it_dropped() {
        let now = 0;
        let changes: Vec<Change> = (0..500)
            .map(|i| change("agent", &format!("did thing number {i}"), None, 0))
            .collect();
        let text = delta(&changes, now, DELTA_CAP).unwrap();
        assert!(
            text.len() <= DELTA_CAP,
            "{} characters exceeds the {DELTA_CAP} cap",
            text.len()
        );
        assert!(text.contains("earlier updates omitted"), "{text}");
        assert!(
            text.contains("did thing number 0"),
            "the newest survives: {text}"
        );
    }

    #[test]
    fn repeated_writes_to_one_key_collapse_to_the_latest() {
        let now = 0;
        let changes = [
            change("a", "wrote notes (v3)", Some("notes"), 0),
            change("a", "wrote notes (v2)", Some("notes"), -1),
            change("a", "wrote notes (v1)", Some("notes"), -2),
            change("a", "wrote other (v1)", Some("other"), -3),
        ];
        let text = delta(&changes, now, DELTA_CAP).unwrap();
        assert_eq!(text.matches("wrote notes").count(), 1, "{text}");
        assert!(text.contains("wrote notes (v3)"), "the latest wins: {text}");
        assert!(text.contains("wrote other"));
    }

    #[test]
    fn a_full_digest_orients_without_instructing() {
        let orientation = Orientation {
            workspace: "api".into(),
            task_brief: Some("port the auth module".into()),
            siblings: vec![Sibling {
                label: "frontend".into(),
                status: "running".into(),
                task: Some("rebuild the login form".into()),
            }],
            entries: vec!["auth/jwt".into(), "db/pool".into()],
            unread: 2,
        };
        let text = full(&orientation, FULL_CAP);
        assert!(text.contains("Workspace: api"));
        assert!(text.contains("frontend (running) — rebuild the login form"));
        assert!(text.contains("- auth/jwt"));
        assert!(text.contains("2 unread messages waiting"));
        assert!(text.len() <= FULL_CAP);
    }

    #[test]
    fn one_unread_message_reads_as_one() {
        let orientation = Orientation {
            workspace: "api".into(),
            unread: 1,
            ..Default::default()
        };
        assert!(full(&orientation, FULL_CAP).contains("1 unread message waiting"));
    }

    #[test]
    fn a_full_digest_keeps_the_orientation_when_keys_overflow() {
        let orientation = Orientation {
            workspace: "api".into(),
            task_brief: Some("port the auth module".into()),
            entries: (0..500).map(|i| format!("namespace/key-{i}")).collect(),
            ..Default::default()
        };
        let text = full(&orientation, FULL_CAP);
        assert!(text.len() <= FULL_CAP);
        assert!(text.contains("Workspace: api"), "{text}");
        assert!(text.contains("port the auth module"));
        assert!(text.contains("earlier updates omitted"));
    }

    #[test]
    fn relative_times_stay_coarse() {
        let now = 10_000_000;
        assert_eq!(ago(now, now), "just now");
        assert_eq!(ago(now, now - 30_000), "just now");
        assert_eq!(ago(now, now - 60_000), "1 minute ago");
        assert_eq!(ago(now, now - 900_000), "15 minutes ago");
        assert_eq!(ago(now, now - 7_200_000), "2 hours ago");
        assert_eq!(
            ago(now, now + 5_000),
            "just now",
            "clock skew is not negative"
        );
    }
}
