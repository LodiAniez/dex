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

// The caps (PRD §10.3) are arguments, not constants: they are configurable, and
// their shipped values live with the rest of the defaults in `platform::config`.

/// One other agent in the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sibling {
    pub label: String,
    pub status: String,
    pub task: Option<String>,
}

/// The agent a digest is for, so it knows who it is: its own row in
/// `agents_list`, and the name others use for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Me {
    pub id: String,
    pub label: Option<String>,
}

/// One of the workspace's repos, where this workspace has it checked out -
/// its worktree, which may be on another branch than the repo's own checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCheckout {
    pub repo: String,
    pub path: Option<String>,
    pub branch: Option<String>,
}

/// What a full digest is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Orientation {
    pub workspace: String,
    pub me: Option<Me>,
    /// The workspace's repos as `(name, branch)`, so an agent knows which
    /// checkout it is in before it touches anything (PRD §10.3).
    pub repos: Vec<WorkspaceCheckout>,
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

/// How a question reaches the owner, stated as the fact it is: the digest
/// orients, it does not instruct. An agent that ends a reply "Reply yes and I'll
/// overwrite the file" has, to Claude Code, finished; the dialog tool fires a
/// hook, so Dex shows it at once and for certain.
const HOW_THE_OWNER_HEARS: &str = "The owner sees at once when an agent asks them something with the AskUserQuestion tool; a question that only ends a reply is easy for them to miss. An agent started by another agent has that agent to ask first.";

/// An agent's orientation when its session starts.
///
/// Never cut: where it is, who it is, and above all its task, which nothing
/// else can tell it (issue #55) - a brief longer than the budget is still
/// delivered whole. The rest fits in what they leave, most important first.
pub fn full(orientation: &Orientation, cap: usize) -> String {
    let mut text = HEADING.to_owned();
    let mut always = vec![format!("Workspace: {}", orientation.workspace)];
    if let Some(me) = &orientation.me {
        always.push(match &me.label {
            Some(label) => format!("This agent is {label} (agent id {}).", me.id),
            None => format!("This agent's id is {}.", me.id),
        });
    }
    if let Some(task) = &orientation.task_brief {
        always.push(format!("This agent's task: {task}"));
    }
    for line in &always {
        text.push('\n');
        text.push_str(line);
    }

    let mut lines = Vec::new();
    for checkout in &orientation.repos {
        let branch = checkout.branch.as_deref().unwrap_or("a detached HEAD");
        // Where: the repo's own checkout may be elsewhere, on another branch.
        lines.push(match &checkout.path {
            Some(path) => format!(
                "Repo {} is on {branch}, checked out at {path}.",
                checkout.repo
            ),
            None => format!("Repo {} is on {branch}.", checkout.repo),
        });
    }
    lines.push(HOW_THE_OWNER_HEARS.to_owned());
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
    let room = cap.saturating_sub(text.len());
    for line in pack(&lines, room, left_out) {
        text.push('\n');
        text.push_str(&line);
    }
    text
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
    fit(&lines, cap)
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

/// Joins lines under the heading within `cap` characters, noting any dropped.
fn fit(lines: &[String], cap: usize) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    let mut text = HEADING.to_owned();
    for line in pack(lines, cap.saturating_sub(text.len()), earlier) {
        text.push('\n');
        text.push_str(&line);
    }
    Some(text)
}

/// The lines, in order, that fit in `room` characters (each with its line
/// break), and then - if any did not - a note of how many were left out, room
/// for which is kept.
fn pack(lines: &[String], room: usize, omission: fn(usize) -> String) -> Vec<String> {
    let reserve = omission(lines.len()).len() + 1;
    let mut used = 0;
    let mut kept = Vec::new();
    for line in lines {
        if used + 1 + line.len() > room.saturating_sub(reserve) {
            break;
        }
        used += 1 + line.len();
        kept.push(line.clone());
    }
    let dropped = lines.len() - kept.len();
    if dropped > 0 {
        kept.push(omission(dropped));
    }
    kept
}

/// What a delta left out: older updates.
fn earlier(dropped: usize) -> String {
    format!("[… {dropped} earlier updates omitted; context_search finds them …]")
}

/// What a full digest left out: the rest of its own lines.
fn left_out(dropped: usize) -> String {
    format!(
        "[… {dropped} more lines of workspace context left out; agents_list and context_search have them …]"
    )
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
mod tests;
