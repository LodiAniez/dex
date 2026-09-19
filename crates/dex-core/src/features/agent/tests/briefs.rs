//! A brief has to reach the agent whole: one too long for that is refused at
//! the spawn, where the lead can still act on it (issue #55).

use dex_protocol::agent::SpawnArgs;

use super::pane;
use crate::features::agent::{AgentError, spawn};

fn brief(task: String, pane: &str) -> SpawnArgs {
    SpawnArgs {
        task,
        repo: None,
        worktree: None,
        label: None,
        direction: None,
        pane: Some(pane.into()),
        workspace: None,
    }
}

#[tokio::test]
async fn a_brief_too_long_to_reach_the_agent_whole_is_refused() {
    let (_dir, state, first) = pane().await;
    let refused = spawn(&state, brief("r".repeat(8_001), &first)).await;
    assert!(
        matches!(refused, Err(AgentError::BriefTooLong { chars: 8_001, .. })),
        "{refused:?}"
    );
}
