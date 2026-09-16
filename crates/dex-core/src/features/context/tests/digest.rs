//! Digest delivery (PRD §10.3), including the mandatory regression: an empty
//! delta produces nothing at all, not "no updates".

use dex_protocol::context::{DigestArgs, NoteArgs};

use super::{from, second_pane, start_agent, workspace_at};
use crate::features::context::{digest, note};

fn full(caller: dex_protocol::context::Caller) -> DigestArgs {
    DigestArgs {
        kind: "full".into(),
        rate_limited: false,
        max_chars: None,
        caller,
    }
}

fn delta(caller: dex_protocol::context::Caller, rate_limited: bool) -> DigestArgs {
    DigestArgs {
        kind: "delta".into(),
        rate_limited,
        max_chars: None,
        caller,
    }
}

async fn say(state: &crate::app::AppState, pane: &str, body: &str) {
    note(
        state,
        NoteArgs {
            body: body.into(),
            tags: None,
            caller: from(pane),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn one_agents_note_reaches_another_agents_next_delta() {
    let (_root, _dir, state, a) = workspace_at().await;
    let b = second_pane(&state, &a).await;
    start_agent(&state, &a, "s-a").await;
    start_agent(&state, &b, "s-b").await;

    say(&state, &a, "the auth module now uses JWT").await;

    let text = digest(&state, delta(from(&b), false))
        .await
        .unwrap()
        .text
        .expect("B should be told");
    assert!(text.contains("the auth module now uses JWT"), "{text}");
}

#[tokio::test]
async fn an_empty_delta_produces_nothing_at_all() {
    let (_root, _dir, state, a) = workspace_at().await;
    let b = second_pane(&state, &a).await;
    start_agent(&state, &a, "s-a").await;
    start_agent(&state, &b, "s-b").await;

    // Nothing has happened yet.
    assert_eq!(
        digest(&state, delta(from(&b), false)).await.unwrap().text,
        None
    );

    // And nothing new since the last delta was delivered.
    say(&state, &a, "something").await;
    assert!(
        digest(&state, delta(from(&b), false))
            .await
            .unwrap()
            .text
            .is_some()
    );
    assert_eq!(
        digest(&state, delta(from(&b), false)).await.unwrap().text,
        None,
        "a delta must never repeat what it already delivered"
    );
}

#[tokio::test]
async fn an_agent_is_never_told_about_its_own_events() {
    let (_root, _dir, state, a) = workspace_at().await;
    let b = second_pane(&state, &a).await;
    start_agent(&state, &a, "s-a").await;
    start_agent(&state, &b, "s-b").await;

    say(&state, &b, "B did this itself").await;
    assert_eq!(
        digest(&state, delta(from(&b), false)).await.unwrap().text,
        None
    );
}

#[tokio::test]
async fn a_session_start_does_not_mute_the_first_batch_delta() {
    let (_root, _dir, state, a) = workspace_at().await;
    let b = second_pane(&state, &a).await;
    start_agent(&state, &a, "s-a").await;
    start_agent(&state, &b, "s-b").await;

    // B orients itself, as its SessionStart hook does.
    assert!(digest(&state, full(from(&b))).await.unwrap().text.is_some());

    // An autonomous agent gets its sibling's note on the very next tool batch,
    // rather than waiting out a rate limit the full digest started.
    say(&state, &a, "a sibling note").await;
    let batch = digest(&state, delta(from(&b), true)).await.unwrap().text;
    assert!(
        batch.is_some_and(|text| text.contains("a sibling note")),
        "a full digest must not start the PostToolBatch clock"
    );
}

#[tokio::test]
async fn a_second_batch_delta_is_rate_limited_but_a_prompt_is_not() {
    let (_root, _dir, state, a) = workspace_at().await;
    let b = second_pane(&state, &a).await;
    start_agent(&state, &a, "s-a").await;
    start_agent(&state, &b, "s-b").await;

    say(&state, &a, "first").await;
    assert!(
        digest(&state, delta(from(&b), true))
            .await
            .unwrap()
            .text
            .is_some()
    );

    // A batch moments later is suppressed, even though there is news.
    say(&state, &a, "second").await;
    assert_eq!(
        digest(&state, delta(from(&b), true)).await.unwrap().text,
        None,
        "at most one PostToolBatch delta per minute"
    );

    // The human typing is not rate limited, and still sees the news.
    let prompt = digest(&state, delta(from(&b), false)).await.unwrap().text;
    assert!(
        prompt
            .as_deref()
            .is_some_and(|text| text.contains("second")),
        "{prompt:?}"
    );
}

#[tokio::test]
async fn a_prompt_delta_does_not_start_the_batch_clock() {
    let (_root, _dir, state, a) = workspace_at().await;
    let b = second_pane(&state, &a).await;
    start_agent(&state, &a, "s-a").await;
    start_agent(&state, &b, "s-b").await;

    say(&state, &a, "one").await;
    assert!(
        digest(&state, delta(from(&b), false))
            .await
            .unwrap()
            .text
            .is_some()
    );

    say(&state, &a, "two").await;
    let batch = digest(&state, delta(from(&b), true)).await.unwrap().text;
    assert!(
        batch.is_some_and(|text| text.contains("two")),
        "a human prompt must not mute the agent's own batch deltas"
    );
}

#[tokio::test]
async fn a_full_digest_names_the_workspace_and_the_other_agents() {
    let (_root, _dir, state, a) = workspace_at().await;
    let b = second_pane(&state, &a).await;
    start_agent(&state, &a, "s-a").await;
    start_agent(&state, &b, "s-b").await;

    let text = digest(&state, full(from(&b))).await.unwrap().text.unwrap();
    assert!(text.starts_with("## Workspace context"), "{text}");
    assert!(text.contains("Workspace:"), "{text}");
    assert!(text.contains("Other agents here:"), "{text}");
}
