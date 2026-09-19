//! The digest's rules: what an agent is told, and in how many characters.

use super::*;
use crate::platform::config::DigestSettings;

/// The budgets Dex ships with; the owner may set others.
fn caps() -> DigestSettings {
    DigestSettings::default()
}

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
    assert_eq!(delta(&[], 0, caps().delta_chars), None);
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
    let text = delta(&changes, now, caps().delta_chars).unwrap();
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
    let text = delta(&changes, now, caps().delta_chars).unwrap();
    assert!(
        text.len() <= caps().delta_chars,
        "{} characters exceeds the {} cap",
        text.len(),
        caps().delta_chars
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
    let text = delta(&changes, now, caps().delta_chars).unwrap();
    assert_eq!(text.matches("wrote notes").count(), 1, "{text}");
    assert!(text.contains("wrote notes (v3)"), "the latest wins: {text}");
    assert!(text.contains("wrote other"));
}

#[test]
fn a_full_digest_orients_without_instructing() {
    let orientation = Orientation {
        workspace: "api".into(),
        me: None,
        repos: vec![WorkspaceCheckout {
            repo: "api".into(),
            path: Some("C:/src/api-wt".into()),
            branch: Some("main".into()),
        }],
        task_brief: Some("port the auth module".into()),
        siblings: vec![Sibling {
            label: "frontend".into(),
            status: "running".into(),
            task: Some("rebuild the login form".into()),
        }],
        entries: vec!["auth/jwt".into(), "db/pool".into()],
        unread: 2,
    };
    let text = full(&orientation, caps().full_chars);
    assert!(text.contains("Workspace: api"));
    assert!(
        text.contains("Repo api is on main, checked out at C:/src/api-wt."),
        "{text}"
    );
    assert!(text.contains("frontend (running) — rebuild the login form"));
    assert!(text.contains("- auth/jwt"));
    assert!(text.contains("2 unread messages waiting"));
    assert!(text.len() <= caps().full_chars);
}

#[test]
fn a_full_digest_says_how_the_owner_hears_a_question_as_a_fact_not_an_order() {
    // An agent that ended its turn "Reply yes and I'll overwrite the file"
    // sat unanswered: to Claude Code, and so to Dex, its turn was over. The
    // dialog tool is the one way of asking that Dex hears about for certain.
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some("port the auth module".into()),
        entries: (0..500).map(|i| format!("namespace/key-{i}")).collect(),
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(text.contains("AskUserQuestion"), "{text}");
    // An agent another agent started is not sent to the owner with what its lead can answer.
    assert!(text.contains("started by another agent"), "{text}");
    // Kept when the keys overflow: it is part of the orientation.
    assert!(text.len() <= caps().full_chars);
    let lower = text.to_lowercase();
    for order in ["you must", "you should", "always ", "never "] {
        assert!(!lower.contains(order), "{order:?} in {text}");
    }
}

#[test]
fn one_unread_message_reads_as_one() {
    let orientation = Orientation {
        workspace: "api".into(),
        unread: 1,
        ..Default::default()
    };
    assert!(full(&orientation, caps().full_chars).contains("1 unread message waiting"));
}

#[test]
fn a_full_digest_keeps_the_orientation_when_keys_overflow() {
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some("port the auth module".into()),
        entries: (0..500).map(|i| format!("namespace/key-{i}")).collect(),
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(text.len() <= caps().full_chars);
    assert!(text.contains("Workspace: api"), "{text}");
    assert!(text.contains("port the auth module"));
    // What was left out is named for what it is: lines of this digest, not updates.
    assert!(
        text.contains("more lines of workspace context left out"),
        "{text}"
    );
    assert!(!text.contains("earlier updates"), "{text}");
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

#[test]
fn a_brief_longer_than_the_budget_is_delivered_whole() {
    // Issue #55: the brief was cut with everything after it; nothing else can
    // tell an agent what it is for.
    let brief = "Review PR 33 carefully. ".repeat(120);
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some(brief.clone()),
        siblings: vec![Sibling {
            label: "frontend".into(),
            status: "running".into(),
            task: None,
        }],
        entries: (0..40).map(|i| format!("notes/entry-{i}")).collect(),
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(brief.len() > caps().full_chars);
    assert!(
        text.contains(&format!("This agent's task: {brief}")),
        "{text}"
    );
    assert!(text.contains("Workspace: api"));
}

#[test]
fn the_rest_of_the_digest_fits_in_what_the_brief_leaves() {
    let brief = "x".repeat(1_500);
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some(brief.clone()),
        entries: (0..100).map(|i| format!("notes/entry-{i}")).collect(),
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(text.len() <= caps().full_chars, "{} > cap", text.len());
    assert!(text.contains(&brief));
    assert!(
        text.contains("more lines of workspace context left out"),
        "{text}"
    );
}

#[test]
fn an_agent_is_told_who_it_is() {
    let named = Orientation {
        workspace: "api".into(),
        me: Some(Me {
            id: "0c23-agent".into(),
            label: Some("reviewer-x".into()),
        }),
        ..Default::default()
    };
    assert!(
        full(&named, caps().full_chars).contains("This agent is reviewer-x (agent id 0c23-agent).")
    );
    let unnamed = Orientation {
        me: Some(Me {
            id: "0c23-agent".into(),
            label: None,
        }),
        ..named
    };
    assert!(full(&unnamed, caps().full_chars).contains("This agent's id is 0c23-agent."));
}
