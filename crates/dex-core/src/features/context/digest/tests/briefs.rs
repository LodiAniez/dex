//! The brief, who the agent is, and what else fits beside them (issue #55).

use super::super::*;
use super::{budgeted, caps};

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

#[test]
fn another_agents_task_is_its_first_line_with_anything_in_it() {
    let with = |task: &str| Orientation {
        workspace: "api".into(),
        siblings: vec![Sibling {
            label: "w".into(),
            status: "idle".into(),
            task: Some(task.into()),
        }],
        ..Default::default()
    };
    // The agent's line, wherever it falls in the digest.
    let line = |task: &str| {
        full(&with(task), 2_000)
            .lines()
            .find(|line| line.starts_with("- w "))
            .map(str::to_owned)
    };
    assert_eq!(
        line("\n\n  fix the bug  \n").as_deref(),
        Some("- w (idle) — fix the bug")
    );
    assert_eq!(
        line("first\nsecond").as_deref(),
        Some("- w (idle) — first…")
    );
    assert_eq!(line("   ").as_deref(), Some("- w (idle)"));
}
