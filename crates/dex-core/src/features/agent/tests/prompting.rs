//! `agent.prompt`: typing the owner's turn into an agent's terminal, and above
//! all when not to. Text typed where no Claude Code is listening is run by the
//! shell.

use dex_protocol::agent::{AgentStatus, PromptAgentArgs, SpawnArgs};

use super::{agents, fire, pane, session};
use crate::features::agent::prompt::{PromptRefusal, one_line, prompt_refusal};
use crate::features::agent::{AgentError, prompt, spawn};
use crate::features::workspace::WorkspaceError;

#[test]
fn only_a_started_agent_at_its_prompt_or_working_may_be_typed_at() {
    assert_eq!(prompt_refusal(AgentStatus::Idle, true, true), None);
    assert_eq!(prompt_refusal(AgentStatus::Running, true, true), None);
}

#[test]
fn an_agent_whose_claude_code_has_not_started_is_a_bare_shell() {
    // The row exists from the moment of the hire; Claude Code a few seconds
    // later. Until then the text would be the shell's to run.
    assert_eq!(
        prompt_refusal(AgentStatus::Idle, false, true),
        Some(PromptRefusal::NotStarted)
    );
}

#[test]
fn an_agent_at_a_dialog_would_have_the_dialog_answered() {
    assert_eq!(
        prompt_refusal(AgentStatus::Waiting, true, true),
        Some(PromptRefusal::WaitingForOwner)
    );
}

#[test]
fn an_agent_nobody_can_vouch_for_is_not_typed_at() {
    // Stopped with an error, or gone quiet: Claude Code may have crashed back
    // to the shell prompt.
    for status in [AgentStatus::Error, AgentStatus::Unknown] {
        assert_eq!(
            prompt_refusal(status, true, true),
            Some(PromptRefusal::OutOfReach),
            "{status:?}"
        );
    }
    assert_eq!(
        prompt_refusal(AgentStatus::Idle, true, false),
        Some(PromptRefusal::NoPane)
    );
}

#[test]
fn a_prompt_is_one_line_because_a_newline_in_a_terminal_is_enter() {
    assert_eq!(
        one_line("  first do this\nthen that\r\n\r\nand report  "),
        "first do this then that and report"
    );
    assert_eq!(one_line(" \n "), "");
}

fn args(agent: &str, text: &str) -> PromptAgentArgs {
    PromptAgentArgs {
        agent: agent.into(),
        text: text.into(),
        from_pane: None,
        workspace: None,
    }
}

#[tokio::test]
async fn a_hire_that_has_not_started_is_refused_rather_than_typed_into() {
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("lead")).await;
    let child = spawn(
        &state,
        SpawnArgs {
            task: "count to ten".into(),
            repo: None,
            worktree: None,
            label: None,
            direction: None,
            pane: Some(first),
            workspace: None,
        },
    )
    .await
    .unwrap();

    let refused = prompt(&state, args(&child.agent, "rm -r build")).await;

    assert!(
        matches!(
            refused,
            Err(AgentError::NotPromptable(PromptRefusal::NotStarted))
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_started_agent_is_typed_at_and_an_empty_prompt_is_not_sent() {
    let (_dir, state, first) = pane().await;
    fire(&state, "session-start", &first, 1, session("mine")).await;
    let id = agents(&state).await[0].id.clone();
    assert!(
        agents(&state).await[0].started,
        "the view says it has started"
    );

    let nothing = prompt(&state, args(&id, " \n ")).await;
    assert!(
        matches!(nothing, Err(AgentError::EmptyPrompt)),
        "{nothing:?}"
    );

    // Test panes have no shell, so reaching the pane proves it passed the gate.
    let sent = prompt(&state, args(&id, "run the tests")).await;
    assert!(
        matches!(
            sent,
            Err(AgentError::Target(WorkspaceError::PaneNotStarted(_)))
        ),
        "{sent:?}"
    );
}
