//! A pane's shell must not inherit the Claude Code session Dex was started from.

use portable_pty::CommandBuilder;

/// What marks a process as running inside a particular Claude Code session.
/// Named one by one: the owner's own settings for Claude Code (`CLAUDE_CONFIG_DIR`,
/// `ANTHROPIC_*`, provider switches) share the prefix and must get through.
const CLAUDE_SESSION_VARS: [&str; 10] = [
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
];

/// Drops the identity of whatever Claude Code session Dex was started from.
///
/// A pane's shell inherits Dex's environment. Started from a terminal inside
/// Claude Code, that includes the session's own variables, and every `claude`
/// run in a pane would take itself for that session's child.
pub(super) fn forget_claude_session(cmd: &mut CommandBuilder) {
    for name in CLAUDE_SESSION_VARS {
        cmd.env_remove(name);
    }
}
