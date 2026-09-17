//! What an agent is waiting for, read out of the hook that said it was waiting.

use serde_json::json;

use super::{agents, fire, pane, session};
use crate::features::agent::reason::waiting_reason;

#[test]
fn a_permission_request_names_the_tool_and_what_it_is_aimed_at() {
    let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "C:/src/legacy/client.rs", "old_string": "a" } });
    assert_eq!(
        waiting_reason(&edit).as_deref(),
        Some("permission to Edit C:/src/legacy/client.rs")
    );
    let bash =
        json!({ "tool_name": "Bash", "tool_input": { "command": "cargo test --workspace" } });
    assert_eq!(
        waiting_reason(&bash).as_deref(),
        Some("permission to Bash: cargo test --workspace")
    );
}

#[test]
fn a_tool_with_nothing_recognisable_in_its_input_is_still_named() {
    let odd = json!({ "tool_name": "mcp__dex__message_inbox", "tool_input": {} });
    assert_eq!(
        waiting_reason(&odd).as_deref(),
        Some("permission to mcp__dex__message_inbox")
    );
}

#[test]
fn a_notification_says_what_it_says() {
    let note = json!({ "message": "Claude needs your permission to use Bash" });
    assert_eq!(
        waiting_reason(&note).as_deref(),
        Some("Claude needs your permission to use Bash")
    );
}

#[test]
fn it_is_one_short_line_whatever_the_agent_was_about_to_run() {
    let long = json!({ "tool_name": "Bash", "tool_input": { "command": format!("echo one\necho {}", "x".repeat(400)) } });
    let reason = waiting_reason(&long).unwrap();
    assert!(!reason.contains('\n'));
    assert!(reason.chars().count() <= 120, "{}", reason.chars().count());
    assert!(reason.ends_with('…'));
}

#[test]
fn a_hook_that_says_nothing_useful_gives_no_reason() {
    assert_eq!(waiting_reason(&json!({ "session_id": "s" })), None);
}

#[tokio::test]
async fn the_reason_reaches_the_agents_view_and_goes_when_the_wait_is_over() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, session("s1")).await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    fire(
        &state,
        "permission",
        &pane,
        3,
        json!({ "session_id": "s1", "tool_name": "Edit", "tool_input": { "file_path": "src/main.rs" } }),
    )
    .await;
    let waiting = &agents(&state).await[0];
    assert_eq!(
        waiting.status_detail.as_deref(),
        Some("permission to Edit src/main.rs")
    );

    fire(&state, "batch", &pane, 4, session("s1")).await;
    assert_eq!(
        agents(&state).await[0].status_detail,
        None,
        "answered: nothing to wait for"
    );
}
