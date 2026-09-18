//! An agent that ends its turn by asking the owner something is idle to
//! Claude Code - the turn is over - and waiting to everyone else.

use dex_protocol::agent::{AgentStatus, ListAgentsArgs};
use serde_json::json;

use super::{agents, fire, pane};
use crate::features::agent::asking::{idle_detail, question_for_owner};
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
fn a_question_is_a_question_without_its_question_mark() {
    // The owner's test: "ask me a question, but use a period instead of a question mark."
    for message in [
        "If you could instantly master any skill without practice, what would it be.",
        "Would you like the long version or the short one.",
        "Which of the two do you prefer.",
        "The draft is ready. How should I name the branch.",
        "Do you want me to push it.",
        "Can you give me the staging URL.",
    ] {
        assert!(question_for_owner(message).is_some(), "{message}");
    }
}

#[test]
fn asking_for_the_go_ahead_is_a_question_however_it_is_put() {
    // Missed when the detector was run over 247 real closing messages from this machine.
    for message in [
        "Version is bumped to 0.1.4 with release notes drafted. Say the word and I'll commit, push, and publish.",
        "Tag, publish with short release notes. Say go and I'll do it, or push and tag yourself if you'd rather.",
        "Nothing is committed. Tell me first whether you want help with recovery; M5 can wait.",
        "I'd number the next release 0.3.0; tell me when you want it published.",
        "I'm still not running anything; it's your call on the recovery steps.",
        "The three agents are still sitting idle in their panes; say if you want me to stop them.",
        "If beta is meant to do real work, send me the brief.",
    ] {
        assert!(question_for_owner(message).is_some(), "{message}");
    }
}

#[test]
fn an_offer_that_blocks_nothing_is_not_a_question() {
    // The agent has finished; it is not waiting to hear.
    for message in [
        "The three merged branches are still there locally and on GitHub; I can delete them if you want.",
        "Dex doesn't do that today. I can build it if you want it.",
        "I can fold these fixes into the PRD if you'd like.",
    ] {
        assert_eq!(question_for_owner(message), None, "{message}");
    }
}

#[test]
fn a_cue_is_whole_words_not_letters_that_happen_to_be_inside_others() {
    for message in [
        "I would say good things about this design.",
        "Set your callback URL in the config and restart.",
        "The endpoint will reply with 404 for an unknown id.",
        "They need yourself-hosted runners for that.",
    ] {
        assert_eq!(question_for_owner(message), None, "{message}");
    }
    // And the real thing still asks.
    assert!(question_for_owner("Reply with \"yes\" and I'll continue.").is_some());
    assert!(question_for_owner("It's your call.").is_some());
}

#[test]
fn a_closing_list_is_read_line_by_line_not_as_one_long_sentence() {
    // A summary in bullets, one of which happens to hold a cue: the turn is over.
    let summary = "Done. What changed:
- The importer no longer needs your API key in the URL
- Retries are capped at three
- All 14 tests pass";
    assert_eq!(question_for_owner(summary), None);
    // A list that ends on the ask shows the ask, not the top of the list.
    let asks = "Two ways to do it:
- a lock file
- a lease in the database
Which would you like?";
    assert_eq!(
        question_for_owner(asks).as_deref(),
        Some("Which would you like?")
    );
}

#[test]
fn a_question_wrapped_over_two_lines_is_shown_whole() {
    // Some agents hard-wrap their prose. A line end is only a boundary after a
    // sentence has ended, or before a list item.
    let wrapped = "The migration is written but not run.\nShould I run it against\nstaging first?";
    assert_eq!(
        question_for_owner(wrapped).as_deref(),
        Some("Should I run it against staging first?")
    );
}

#[test]
fn a_number_that_opens_a_question_is_part_of_it() {
    assert_eq!(
        question_for_owner("2 or 3 replicas?").as_deref(),
        Some("2 or 3 replicas?")
    );
    // A list marker is not.
    assert_eq!(
        question_for_owner("Options:\n1. a lock file\n2) a lease\n- Which would you like?")
            .as_deref(),
        Some("Which would you like?")
    );
}

#[test]
fn a_statement_that_only_mentions_a_question_word_is_not_a_question() {
    for message in [
        "I checked what it would be under load, and it holds.",
        "When the tests ran, two failed and I fixed both.",
        "What I did was rename the module.",
        "How it works is documented in the README.",
        "That is why the retry is there.",
        "I can do the rest tomorrow.",
        "It would be faster with a cache.",
    ] {
        assert_eq!(question_for_owner(message), None, "{message}");
    }
}

#[test]
fn an_idle_agent_always_says_how_its_turn_ended() {
    assert_eq!(
        idle_detail(&json!({ "last_assistant_message": "Should I run it?" })).as_deref(),
        Some("asked you: Should I run it?")
    );
    // Not a question: what it said last, so the owner can see for themselves.
    assert_eq!(
        idle_detail(&json!({ "last_assistant_message": "I renamed the module.

All 14 tests pass. Nothing else was touched." }))
        .as_deref(),
        Some("said: All 14 tests pass. Nothing else was touched.")
    );
    assert_eq!(
        idle_detail(&json!({ "last_assistant_message": "  " })),
        None
    );
    assert_eq!(idle_detail(&json!({})), None);
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
    assert_eq!(
        agent.status_detail.as_deref(),
        Some("said: Ran it. All green.")
    );
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

#[tokio::test]
async fn what_an_agent_merely_said_is_not_put_in_the_workspace_log() {
    // Every finished turn would otherwise copy its last words to every other agent's digest.
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
        stop_saying("The token is hunter2. All done."),
    )
    .await;
    let log = crate::features::context::events(
        &state,
        dex_protocol::context::ScopeArgs::for_caller(dex_protocol::context::Caller {
            pane: Some(pane.clone()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert!(
        log.events
            .iter()
            .all(|event| !event.body.contains("hunter2")),
        "{:?}",
        log.events
    );
    assert!(
        log.events
            .iter()
            .any(|event| event.body.ends_with("is idle")),
        "{:?}",
        log.events
    );
}
