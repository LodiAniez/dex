//! The digest's rules: what an agent is told, and in how many characters.

use super::*;
use crate::platform::config::DigestSettings;

/// A digest's length without its task line: the task is outside the budget.
fn budgeted(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.starts_with("This agent's task: "))
        .map(|line| line.len() + 1)
        .sum::<usize>()
        .saturating_sub(1)
}

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
    assert!(budgeted(&text) <= caps().full_chars);
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
    assert!(budgeted(&text) <= caps().full_chars);
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
fn everything_but_the_brief_keeps_to_the_budget() {
    let brief = "x".repeat(1_500);
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some(brief.clone()),
        entries: (0..100).map(|i| format!("notes/entry-{i}")).collect(),
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(
        budgeted(&text) <= caps().full_chars,
        "{} > cap",
        budgeted(&text)
    );
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

#[test]
fn a_long_brief_leaves_the_rest_its_whole_budget() {
    // The brief is outside the budget: an agent with a long one still learns
    // its repos, whom to ask, and its messages.
    let orientation = Orientation {
        workspace: "api".into(),
        repos: vec![WorkspaceCheckout {
            repo: "api".into(),
            path: Some("C:/src/api".into()),
            branch: Some("main".into()),
        }],
        task_brief: Some("y".repeat(3_000)),
        unread: 1,
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(text.contains("Repo api is on main"), "{text}");
    assert!(text.contains("has that agent to ask first"), "{text}");
    assert!(text.contains("1 unread message waiting"), "{text}");
}

#[test]
fn a_digest_never_passes_what_claude_code_shows_whole() {
    // Past 10,000 characters Claude Code shows the agent a file path and a
    // preview instead: a brief that long is cut, and says where the rest is.
    let brief = "z".repeat(20_000);
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some(brief),
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(
        text.chars().count() <= CEILING,
        "{} chars",
        text.chars().count()
    );
    assert!(
        text.contains("The brief continues"),
        "{}",
        text.chars().rev().take(200).collect::<String>()
    );
    assert!(text.contains("agents_list"));
}

#[test]
fn the_longest_brief_a_spawn_takes_arrives_whole_even_with_a_full_workspace() {
    // The brief wins: the rest shrinks to fit beside it under the ceiling.
    let brief = "b".repeat(8_000);
    let orientation = Orientation {
        workspace: "api".into(),
        me: Some(Me {
            id: "0c23c7a1-5f44-4d7e-9a57-5c0a2f1b9e10".into(),
            label: Some("reviewer-x".into()),
        }),
        task_brief: Some(brief.clone()),
        entries: (0..200).map(|i| format!("notes/entry-{i}")).collect(),
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(
        text.contains(&format!("This agent's task: {brief}\n")),
        "the brief was cut"
    );
    assert!(
        text.chars().count() <= CEILING,
        "{} chars",
        text.chars().count()
    );
}

#[test]
fn a_budget_set_past_the_ceiling_still_keeps_under_it() {
    let orientation = Orientation {
        workspace: "api".into(),
        entries: (0..2_000).map(|i| format!("notes/entry-{i}")).collect(),
        ..Default::default()
    };
    let text = full(&orientation, 50_000);
    assert!(
        text.chars().count() <= CEILING,
        "{} chars",
        text.chars().count()
    );
}

#[test]
fn a_brief_in_another_script_is_counted_in_characters_not_bytes() {
    // 3,000 characters of CJK are 9,000 bytes: the rest must not be squeezed out.
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some("審".repeat(3_000)),
        unread: 1,
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(
        text.contains("1 unread message waiting"),
        "the rest was squeezed out"
    );
    assert!(text.chars().count() <= CEILING);
}

#[test]
fn another_agents_long_brief_does_not_crowd_out_the_rest() {
    let orientation = Orientation {
        workspace: "api".into(),
        siblings: vec![Sibling {
            label: "lead".into(),
            status: "running".into(),
            task: Some("a very long brief. ".repeat(400)),
        }],
        entries: vec!["notes/plan".into()],
        unread: 2,
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(
        text.contains("- lead (running) — a very long brief."),
        "{text}"
    );
    assert!(text.contains("- notes/plan"), "{text}");
    assert!(text.contains("2 unread messages waiting"), "{text}");
}

#[test]
fn unread_messages_are_told_even_beside_the_longest_brief() {
    let orientation = Orientation {
        workspace: "api".into(),
        task_brief: Some("b".repeat(8_000)),
        siblings: (0..10)
            .map(|i| Sibling {
                label: format!("worker-{i}"),
                status: "running".into(),
                task: Some("x".repeat(200)),
            })
            .collect(),
        entries: (0..25).map(|i| format!("notes/entry-{i}")).collect(),
        unread: 3,
        ..Default::default()
    };
    let text = full(&orientation, caps().full_chars);
    assert!(text.contains("3 unread messages waiting"), "{text}");
}
