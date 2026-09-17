//! An agent that ends its turn by asking the owner something is idle to
//! Claude Code - the turn is over - and waiting to everyone else.

use dex_protocol::agent::{AgentStatus, ListAgentsArgs};
use serde_json::json;

use super::{agents, fire, pane};
use crate::features::agent::asking::question_for_owner;
use crate::features::agent::list;

#[test]
fn a_turn_that_ends_by_asking_for_a_yes_is_a_question() {
    // What the agent in the bug report actually said, twice.
    let first = "I have not rewritten the file. I am not treating it as a confirmed requirement on the lead's word alone.\n\nIf you do want the two-sentence version, tell me and I'll rewrite the file in place, keeping the core view, and post the note. Otherwise the essay stays as it is.";
    assert!(question_for_owner(first).is_some(), "tell me and");
    let second = "Here are the two sentences.\n\nReply \"yes\" and I'll overwrite the file with exactly those two sentences and post the note. Otherwise the full essay stays in place.";
    let asked = question_for_owner(second).unwrap();
    assert!(asked.starts_with("Reply \"yes\""), "{asked}");
}

#[test]
fn a_closing_question_is_a_question() {
    for message in [
        "The migration is written but not run. Should I run it against staging?",
        "Two options: a lock file or a lease. Which would you like?",
        "Done with the draft.\n\nDo you want me to open a PR?",
        "I need your approval before deleting the old table.",
        "Please confirm the bucket name and I will continue.",
    ] {
        assert!(question_for_owner(message).is_some(), "{message}");
    }
}

#[test]
fn a_finished_turn_is_not_a_question() {
    for message in [
        "done",
        "`essay-1.md` now contains only the two sentences you approved, and I posted the note. No other file was touched.",
        "All 14 tests pass. I confirmed the retry path is covered.",
        "",
    ] {
        assert_eq!(question_for_owner(message), None, "{message}");
    }
}

#[test]
fn a_closing_pleasantry_is_not_a_question() {
    for message in [
        "The port is finished and the tests pass. Let me know if you need anything else.",
        "Pushed. Is there anything else you'd like me to do?",
        "Fixed. Feel free to ask if you have questions.",
    ] {
        assert_eq!(question_for_owner(message), None, "{message}");
    }
}

#[test]
fn a_question_asked_mid_message_and_then_answered_by_the_agent_itself_is_not_one() {
    let message = "Why did the test fail? Because the clock was mocked. I fixed the fixture and everything passes now.";
    assert_eq!(question_for_owner(message), None);
}

#[test]
fn what_was_asked_fits_on_a_line_of_a_narrow_panel() {
    let long = format!(
        "Should I {}?",
        "also rewrite the adjacent module and ".repeat(12)
    );
    let asked = question_for_owner(&long).unwrap();
    assert!(asked.chars().count() <= 120, "{}", asked.chars().count());
    assert!(asked.ends_with('…'));
    assert!(!asked.contains('\n'));
}

fn stop_saying(text: &str) -> serde_json::Value {
    json!({ "session_id": "s1", "last_assistant_message": text })
}

#[tokio::test]
async fn an_agent_that_stops_on_a_question_is_idle_and_says_what_it_asked() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1" }),
    )
    .await;
    fire(&state, "prompt", &pane, 2, json!({ "session_id": "s1" })).await;
    fire(
        &state,
        "stop",
        &pane,
        3,
        stop_saying("Reply \"yes\" and I'll overwrite the file."),
    )
    .await;

    let agent = &agents(&state).await[0];
    // Idle, so that it can still be prompted and woken: answering it is a prompt.
    assert_eq!(agent.status, AgentStatus::Idle);
    assert_eq!(
        agent.status_detail.as_deref(),
        Some("asked you: Reply \"yes\" and I'll overwrite the file.")
    );
}

#[tokio::test]
async fn answering_it_clears_what_it_asked() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1" }),
    )
    .await;
    fire(&state, "prompt", &pane, 2, json!({ "session_id": "s1" })).await;
    fire(&state, "stop", &pane, 3, stop_saying("Should I run it?")).await;
    fire(&state, "prompt", &pane, 4, json!({ "session_id": "s1" })).await;
    assert_eq!(agents(&state).await[0].status_detail, None);
    fire(&state, "stop", &pane, 5, stop_saying("Ran it. All green.")).await;
    let agent = &agents(&state).await[0];
    assert_eq!(agent.status, AgentStatus::Idle);
    assert_eq!(agent.status_detail, None);
}

#[tokio::test]
async fn the_idle_reminder_a_minute_later_does_not_wipe_what_it_asked() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1" }),
    )
    .await;
    fire(&state, "prompt", &pane, 2, json!({ "session_id": "s1" })).await;
    fire(&state, "stop", &pane, 3, stop_saying("Should I run it?")).await;
    fire(&state, "idle", &pane, 4, json!({ "session_id": "s1" })).await;
    assert_eq!(
        agents(&state).await[0].status_detail.as_deref(),
        Some("asked you: Should I run it?")
    );
}

#[tokio::test]
async fn other_agents_are_not_told_what_was_asked() {
    let (_dir, state, pane) = pane().await;
    fire(
        &state,
        "session-start",
        &pane,
        1,
        json!({ "session_id": "s1" }),
    )
    .await;
    fire(&state, "prompt", &pane, 2, json!({ "session_id": "s1" })).await;
    fire(
        &state,
        "stop",
        &pane,
        3,
        stop_saying("Should I delete prod-db-7?"),
    )
    .await;
    let other = crate::features::workspace::split_pane(
        &state,
        dex_protocol::workspace::SplitPaneArgs {
            pane: pane.clone(),
            direction: dex_protocol::workspace::SplitDirection::Right,
            cwd: None,
            label: None,
            kind: None,
        },
    )
    .await
    .unwrap()
    .workspaces[0]
        .active_pane
        .clone()
        .unwrap();
    fire(
        &state,
        "session-start",
        &other,
        4,
        json!({ "session_id": "s2" }),
    )
    .await;

    let seen = list(
        &state,
        ListAgentsArgs {
            pane: Some(other),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let asker = seen
        .agents
        .iter()
        .find(|agent| agent.pane_id.as_deref() == Some(pane.as_str()))
        .unwrap();
    assert_eq!(asker.status_detail, None);
}
