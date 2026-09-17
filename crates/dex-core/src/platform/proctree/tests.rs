//! Whether Claude Code is running under a pane's shell, decided from a
//! snapshot of the process table.

use super::{Presence, Proc, claude_under, parse};

fn proc(pid: u32, parent: u32, name: &str, command: &str) -> Proc {
    Proc {
        pid,
        parent,
        name: name.into(),
        command: command.into(),
    }
}

fn table() -> Vec<Proc> {
    vec![
        proc(100, 1, "powershell.exe", "powershell.exe"),
        proc(110, 100, "claude.exe", "claude --permission-mode auto"),
        proc(111, 110, "node.exe", "node mcp-server.js"),
        // Another pane, whose Claude Code has gone: a bare shell.
        proc(200, 1, "powershell.exe", "powershell.exe"),
        proc(210, 200, "git.exe", "git status"),
        // An npm-installed Claude Code runs as node.
        proc(300, 1, "cmd.exe", "cmd.exe"),
        proc(
            310,
            300,
            "node.exe",
            "node C:/Users/me/AppData/Roaming/npm/node_modules/@anthropic-ai/claude-code/cli.js",
        ),
        // A WSL pane: whatever runs inside is not in this table.
        proc(400, 1, "powershell.exe", "powershell.exe"),
        proc(410, 400, "wsl.exe", "wsl.exe -d Ubuntu"),
    ]
}

#[test]
fn claude_running_under_the_shell_is_there() {
    assert_eq!(claude_under(&table(), 100), Presence::There);
}

#[test]
fn a_shell_with_no_claude_under_it_is_a_bare_shell() {
    // Quit with Ctrl+C, or crashed: no SessionEnd fired, the agent still reads
    // "idle" - and whatever is typed here, the shell runs.
    assert_eq!(claude_under(&table(), 200), Presence::Gone);
}

#[test]
fn an_npm_installed_claude_code_is_recognised_by_its_command_line() {
    assert_eq!(claude_under(&table(), 300), Presence::There);
}

#[test]
fn some_other_node_process_is_not_mistaken_for_it() {
    let procs = vec![
        proc(500, 1, "powershell.exe", "powershell.exe"),
        proc(510, 500, "node.exe", "node server.js"),
    ];
    assert_eq!(claude_under(&procs, 500), Presence::Gone);
}

#[test]
fn a_wsl_pane_cannot_be_seen_into_so_nothing_is_concluded() {
    assert_eq!(claude_under(&table(), 400), Presence::CannotTell);
}

#[test]
fn a_shell_that_is_not_in_the_table_at_all_concludes_nothing() {
    // The snapshot and the pane can disagree for a moment; a refusal needs evidence.
    assert_eq!(claude_under(&table(), 999), Presence::CannotTell);
}

#[test]
fn a_cycle_in_a_corrupt_table_does_not_hang() {
    let procs = vec![proc(1, 2, "a.exe", "a"), proc(2, 1, "b.exe", "b")];
    assert_eq!(claude_under(&procs, 1), Presence::Gone);
}

#[test]
fn the_snapshot_is_parsed_whether_powershell_prints_a_list_or_a_single_object() {
    let many = r#"[{"p":100,"pp":1,"n":"powershell.exe","c":"powershell.exe"},{"p":110,"pp":100,"n":"claude.exe","c":null}]"#;
    let procs = parse(many).unwrap();
    assert_eq!(procs.len(), 2);
    assert_eq!(
        procs[1].command, "",
        "a process with no readable command line"
    );
    let one = r#"{"p":100,"pp":1,"n":"powershell.exe","c":"x"}"#;
    assert_eq!(parse(one).unwrap().len(), 1);
    assert!(parse("not json").is_err());
}
