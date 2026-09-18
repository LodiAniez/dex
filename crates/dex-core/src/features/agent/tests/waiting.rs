//! What an agent is waiting for, read out of the hook that said it was waiting.

use serde_json::json;

use super::{agents, fire, pane, session};
use crate::features::agent::reason::waiting_reason;

#[test]
fn a_permission_request_names_the_tool_and_what_it_is_aimed_at() {
    let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "C:/src/legacy/client.rs", "old_string": "a" } });
    assert_eq!(
        waiting_reason(&edit).as_deref(),
        Some("permission to Edit …/legacy/client.rs")
    );
    let bash =
        json!({ "tool_name": "Bash", "tool_input": { "command": "cargo test --workspace" } });
    assert_eq!(
        waiting_reason(&bash).as_deref(),
        Some("permission to Bash: cargo test --workspace")
    );
}

#[test]
fn the_question_dialog_says_what_was_asked_not_that_a_tool_wants_permission() {
    // Claude Code's AskUserQuestion: a dialog in the terminal, and the one way of
    // asking the owner something that Dex hears about for certain.
    let one = json!({ "tool_name": "AskUserQuestion", "tool_input": { "questions": [
        { "question": "Do you prefer red or blue?", "header": "Colour", "options": [{ "label": "Red" }, { "label": "Blue" }] }
    ] } });
    assert_eq!(
        waiting_reason(&one).as_deref(),
        Some("asked you: Do you prefer red or blue?")
    );
    let two = json!({ "tool_name": "AskUserQuestion", "tool_input": { "questions": [
        { "question": "Which database?" }, { "question": "Which region?" }
    ] } });
    assert_eq!(
        waiting_reason(&two).as_deref(),
        Some("asked you: Which database? (+1 more)")
    );
    // A shape this version does not know: still true, just less said.
    let odd = json!({ "tool_name": "AskUserQuestion", "tool_input": {} });
    assert_eq!(waiting_reason(&odd).as_deref(), Some("asked you something"));
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
    let long = json!({ "message": format!("Claude needs you\nbecause {}", "x ".repeat(300)) });
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

#[tokio::test]
async fn a_second_dialog_in_a_row_replaces_the_first_ones_reason() {
    // The status does not change - waiting, then waiting - so the hook used to
    // be ignored, and the panel went on naming a dialog that was no longer the
    // one on screen. A wrong reason is worse than none.
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, session("s1")).await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    let ask = |tool: &str, key: &str, value: &str| json!({ "session_id": "s1", "tool_name": tool, "tool_input": { key: value } });
    fire(
        &state,
        "permission",
        &pane,
        3,
        ask("Edit", "file_path", "src/client.rs"),
    )
    .await;
    fire(
        &state,
        "permission",
        &pane,
        4,
        ask("Bash", "command", "rm -r build"),
    )
    .await;

    assert_eq!(
        agents(&state).await[0].status_detail.as_deref(),
        Some("permission to Bash: rm -r build")
    );

    // One wait, one line in the activity log: the second dialog is not news
    // about the agent's state.
    let log = crate::features::context::events(
        &state,
        dex_protocol::context::ScopeArgs::for_caller(dex_protocol::context::Caller {
            pane: Some(pane.clone()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let waits = log
        .events
        .iter()
        .filter(|event| event.body.contains("is waiting"))
        .count();
    assert_eq!(waits, 1, "{:?}", log.events);
}

#[tokio::test]
async fn an_older_dialog_delivered_late_does_not_replace_a_newer_ones_reason() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, session("s1")).await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    let ask = |tool: &str| json!({ "session_id": "s1", "tool_name": tool, "tool_input": {} });
    fire(&state, "permission", &pane, 5, ask("Bash")).await;
    fire(&state, "permission", &pane, 4, ask("Edit")).await;

    assert_eq!(
        agents(&state).await[0].status_detail.as_deref(),
        Some("permission to Bash")
    );
}

#[test]
fn a_command_is_named_by_what_it_runs_not_by_what_it_carries() {
    // The reason is stored, shown, and until now handed to other agents. What a
    // command runs is worth saying; its arguments are where the secrets are.
    let reason = |command: &str| {
        waiting_reason(&json!({ "tool_name": "Bash", "tool_input": { "command": command } }))
            .unwrap()
    };
    assert_eq!(
        reason("cargo test --workspace"),
        "permission to Bash: cargo test --workspace"
    );
    assert_eq!(
        reason("curl -H 'Authorization: Bearer sk-live-abc123' https://api.example.test"),
        "permission to Bash: curl -H …"
    );
    assert_eq!(reason("mysql -psecret app"), "permission to Bash: mysql …");
    assert_eq!(
        reason("API_KEY=abc123 node deploy.js"),
        "permission to Bash: …"
    );
    assert_eq!(
        reason("git push https://user:token@github.com/x/y"),
        "permission to Bash: git push …"
    );
    assert_eq!(reason("rm -r build"), "permission to Bash: rm -r build");
    assert_eq!(
        reason("npm run build -- --mode production --verbose"),
        "permission to Bash: npm run build …"
    );
}

#[tokio::test]
async fn another_agent_is_not_told_what_this_one_is_waiting_for() {
    use dex_protocol::agent::ListAgentsArgs;
    let (_dir, state, first) = pane().await;
    let second = crate::features::workspace::split_pane(
        &state,
        dex_protocol::workspace::SplitPaneArgs {
            pane: first.clone(),
            direction: dex_protocol::workspace::SplitDirection::Right,
            cwd: None,
            label: None,
            kind: None,
            runtime: None,
        },
    )
    .await
    .unwrap()
    .workspaces[0]
        .active_pane
        .clone()
        .unwrap();
    fire(&state, "session-start", &first, 1, session("a")).await;
    fire(&state, "session-start", &second, 1, session("b")).await;
    fire(&state, "prompt", &first, 2, session("a")).await;
    fire(
        &state,
        "permission",
        &first,
        3,
        json!({ "session_id": "a", "tool_name": "Edit", "tool_input": { "file_path": "secrets/prod.env" } }),
    )
    .await;

    let seen_by = |pane: Option<String>| {
        let state = state.clone();
        async move {
            crate::features::agent::list(
                &state,
                ListAgentsArgs {
                    pane,
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .agents
            .into_iter()
            .find(|agent| agent.status == dex_protocol::agent::AgentStatus::Waiting)
            .unwrap()
            .status_detail
        }
    };
    // The owner - the office asks with no pane - sees it; so does the agent itself.
    assert!(seen_by(None).await.is_some());
    assert!(seen_by(Some(first.clone())).await.is_some());
    // The agent in the other pane sees that it is waiting, and not what for.
    assert_eq!(seen_by(Some(second)).await, None);
}

#[test]
fn a_long_path_keeps_its_end_which_is_the_part_that_says_which_file() {
    // As captured from Claude Code 2.1.274: absolute, Windows, and long.
    let ask = json!({ "tool_name": "Write", "tool_input": { "file_path": r"C:\Users\Admin\AppData\Local\Temp\claude\scratch\todo-probe\permwork\probe.txt" } });
    assert_eq!(
        waiting_reason(&ask).as_deref(),
        Some("permission to Write …/permwork/probe.txt")
    );
    let short = json!({ "tool_name": "Edit", "tool_input": { "file_path": "src/main.rs" } });
    assert_eq!(
        waiting_reason(&short).as_deref(),
        Some("permission to Edit src/main.rs")
    );
}

#[tokio::test]
async fn the_notification_that_follows_a_dialog_does_not_replace_what_the_dialog_said() {
    // Seen live: PermissionRequest says what is asked; the Notification a few
    // seconds later says only "Claude needs your permission", and used to win.
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, session("s1")).await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    let dialog = json!({ "session_id": "s1", "tool_name": "AskUserQuestion", "tool_input": { "questions": [{ "question": "Red or blue?" }] } });
    fire(&state, "permission", &pane, 3, dialog).await;
    fire(
        &state,
        "waiting",
        &pane,
        4,
        json!({ "session_id": "s1", "message": "Claude needs your permission" }),
    )
    .await;
    assert_eq!(
        agents(&state).await[0].status_detail.as_deref(),
        Some("asked you: Red or blue?")
    );
}

#[tokio::test]
async fn a_notification_still_gives_the_reason_when_nothing_better_is_known() {
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, session("s1")).await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    fire(
        &state,
        "waiting",
        &pane,
        3,
        json!({ "session_id": "s1", "message": "Claude is waiting for your input" }),
    )
    .await;
    assert_eq!(
        agents(&state).await[0].status_detail.as_deref(),
        Some("Claude is waiting for your input")
    );
}

#[tokio::test]
async fn a_later_notification_replaces_an_earlier_one_for_a_wait_no_dialog_ever_described() {
    // Some waits only ever send notifications. With nothing more exact on
    // record, the reason follows the newest of them.
    let (_dir, state, pane) = pane().await;
    fire(&state, "session-start", &pane, 1, session("s1")).await;
    fire(&state, "prompt", &pane, 2, session("s1")).await;
    fire(
        &state,
        "waiting",
        &pane,
        3,
        json!({ "session_id": "s1", "message": "Claude is waiting for your input" }),
    )
    .await;
    fire(
        &state,
        "waiting",
        &pane,
        4,
        json!({ "session_id": "s1", "message": "Claude needs you to choose a login method" }),
    )
    .await;
    assert_eq!(
        agents(&state).await[0].status_detail.as_deref(),
        Some("Claude needs you to choose a login method")
    );
}

#[test]
fn only_what_a_dialog_said_is_kept_against_a_notification() {
    use crate::features::agent::reason::keeps_its_reason;
    assert!(keeps_its_reason(
        true,
        Some("permission to Edit …/legacy/client.rs")
    ));
    assert!(keeps_its_reason(true, Some("asked you: Red or blue?")));
    assert!(keeps_its_reason(true, Some("asked you something")));
    assert!(!keeps_its_reason(
        true,
        Some("Claude is waiting for your input")
    ));
    assert!(!keeps_its_reason(true, None));
    // A dialog always says what it is for, whatever was there.
    assert!(!keeps_its_reason(false, Some("asked you: Red or blue?")));
}
