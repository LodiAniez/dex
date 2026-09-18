//! Which Claude Code events Dex hooks, and how (docs/prd.md §9.3).

/// One hook Dex installs: the Claude Code event (and matcher), the `dex event`
/// kind it runs, and whether Claude Code waits for it. Only hooks whose output
/// must reach the model before it continues (digests, from M6) wait; status
/// updates run in the background (PRD §9.3).
pub(super) struct Hook {
    pub(super) event: &'static str,
    pub(super) matcher: Option<&'static str>,
    pub(super) kind: &'static str,
    pub(super) wait: bool,
}

pub(super) const HOOKS: [Hook; 10] = [
    Hook {
        event: "SessionStart",
        matcher: None,
        kind: "session-start",
        wait: true,
    },
    Hook {
        event: "UserPromptSubmit",
        matcher: None,
        kind: "prompt",
        wait: true,
    },
    Hook {
        event: "PostToolBatch",
        matcher: None,
        kind: "batch",
        wait: true,
    },
    Hook {
        event: "PermissionRequest",
        matcher: None,
        kind: "permission",
        wait: false,
    },
    Hook {
        event: "Notification",
        matcher: Some("permission_prompt"),
        kind: "waiting",
        wait: false,
    },
    Hook {
        event: "Notification",
        matcher: Some("agent_needs_input"),
        kind: "waiting",
        wait: false,
    },
    Hook {
        event: "Notification",
        matcher: Some("idle_prompt"),
        kind: "idle",
        wait: false,
    },
    Hook {
        event: "Stop",
        matcher: None,
        kind: "stop",
        wait: false,
    },
    Hook {
        event: "StopFailure",
        matcher: None,
        kind: "stop-failure",
        wait: false,
    },
    Hook {
        event: "SessionEnd",
        matcher: None,
        kind: "session-end",
        wait: false,
    },
];
