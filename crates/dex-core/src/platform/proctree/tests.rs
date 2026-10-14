//! Whether Claude Code is running under a pane's shell, decided from a
//! snapshot of the process table.

use super::{Presence, Proc, SWEEP_FRESH, Table, bare, claude_under, parse};

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

/// A control character in the text was put there on the way out.
///
/// `ConvertTo-Json` escapes every control character it means to send, so a raw
/// one is damage, not data: PowerShell writes stdout in the console's code
/// page, and a code page without a bullet in it best-fit maps that character
/// onto a byte that happens to be a control one. It costs the command line it
/// landed in a character, and nothing else (issue #91).
#[test]
fn a_byte_mangled_on_the_way_out_costs_one_command_line_not_the_table() {
    let mangled = "[{\"p\":100,\"pp\":1,\"n\":\"powershell.exe\",\"c\":\"powershell.exe\"},\
                    {\"p\":110,\"pp\":100,\"n\":\"git.exe\",\"c\":\"git commit -m \u{7}done\"}]";
    let procs = parse(mangled).expect("one bad byte does not lose every process on the machine");
    assert_eq!(procs.len(), 2);
    assert_eq!(
        procs[0].command, "powershell.exe",
        "the other row is untouched"
    );
    assert_eq!(procs[1].command, "git commit -m done");
    assert_eq!(
        claude_under(&procs, 100),
        Presence::Gone,
        "and the table can still be asked about"
    );
}

#[test]
fn claude_code_under_another_runtime_is_recognised_by_its_command_line() {
    // The watchdog ends whoever is judged gone, so a miss costs an agent:
    // anything under the shell that names Claude Code counts.
    let procs = vec![
        proc(600, 1, "pwsh.exe", "pwsh.exe"),
        proc(
            610,
            600,
            "bun.exe",
            "bun C:/Users/me/.bun/install/global/node_modules/@anthropic-ai/claude-code/cli.js",
        ),
    ];
    assert_eq!(claude_under(&procs, 600), Presence::There);
}

#[test]
fn a_runtime_whose_command_line_cannot_be_read_concludes_nothing() {
    // It may be Claude Code and it may not: that is not evidence that it has gone.
    for runtime in ["node.exe", "bun.exe", "deno.exe"] {
        let procs = vec![
            proc(700, 1, "pwsh.exe", "pwsh.exe"),
            proc(710, 700, runtime, ""),
        ];
        assert_eq!(claude_under(&procs, 700), Presence::CannotTell, "{runtime}");
    }
}

#[test]
fn a_pane_whose_own_process_is_claude_code_has_it() {
    // `shell = "claude"`: nothing runs under it, because it is it.
    let procs = vec![proc(800, 1, "claude.exe", "claude")];
    assert_eq!(claude_under(&procs, 800), Presence::There);
}

#[test]
fn ps_output_is_read_into_the_same_rows_as_the_windows_table() {
    // `ps -A -o pid=,ppid=,args=`: right-aligned numbers, then the command line.
    let text = "    1     0 /sbin/launchd\n  501     1 -zsh\n  502   501 /Users/me/.local/bin/claude --permission-mode auto\n  503   502 node /Users/me/.npm/lib/node_modules/@anthropic-ai/claude-code/cli.js\n";
    let procs = super::parse_ps(text);
    assert_eq!(procs.len(), 4);
    assert_eq!(
        procs[2],
        proc(
            502,
            501,
            "claude",
            "/Users/me/.local/bin/claude --permission-mode auto"
        )
    );
    // A login shell is shown as "-zsh"; its name is still zsh.
    assert_eq!(procs[1].name, "zsh");
    assert_eq!(procs[3].name, "node");
    assert_eq!(claude_under(&procs, 501), Presence::There);
}

#[test]
fn a_line_ps_could_not_fill_in_is_skipped_not_misread() {
    let procs = super::parse_ps("  12\n  abc  1 x\n\n  7   1 /bin/sleep 30\n");
    assert_eq!(procs, vec![proc(7, 1, "sleep", "/bin/sleep 30")]);
}

#[test]
fn a_pane_whose_own_process_is_wsl_cannot_be_seen_into_either() {
    // A WSL pane runs `wsl.exe` as its shell. Nothing Windows can see runs
    // under it, and that is no evidence Claude Code has gone.
    let procs = [
        proc(700, 1, "wsl.exe", "wsl.exe -d Ubuntu --cd C:/src"),
        proc(701, 700, "conhost.exe", "conhost.exe 0x4"),
    ];
    assert_eq!(claude_under(&procs, 700), Presence::CannotTell);
}

#[test]
fn a_shell_with_nothing_under_it_is_bare() {
    let procs = [
        proc(800, 1, "pwsh.exe", "pwsh.exe"),
        proc(801, 800, "conhost.exe", "conhost.exe 0x4"),
        proc(900, 1, "pwsh.exe", "pwsh.exe"),
        proc(901, 900, "node.exe", "node server.js"),
    ];
    assert_eq!(bare(&procs, 800), Some(true), "only its console host");
    assert_eq!(bare(&procs, 900), Some(false), "a server is running in it");
    assert_eq!(bare(&procs, 999), None, "not in the table");
}

/// A process started just now, to look for in a table. Killed by its test.
fn a_child_to_look_for() -> std::process::Child {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd.exe");
        c.args(["/c", "pause"]).stdin(std::process::Stdio::piped());
        c
    } else {
        let mut c = std::process::Command::new("/bin/sh");
        c.args(["-c", "sleep 30; :"]);
        c
    };
    command
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("a child to look for")
}

/// What the sweep needs: two callers, one read of the process table.
///
/// Timed rather than counted, because the read is a subprocess: the second
/// call cannot have started PowerShell and come back in a fraction of the
/// first call's time.
#[test]
fn a_table_read_once_is_handed_to_the_next_caller() {
    use std::time::{Duration, Instant};

    let table = Table::default();

    let first = Instant::now();
    let once = table.within(SWEEP_FRESH).expect("the process table");
    let cold = first.elapsed();

    let again = Instant::now();
    let twice = table.within(SWEEP_FRESH).expect("the table again");
    let warm = again.elapsed();

    assert!(!once.is_empty(), "this process is in it at least");
    assert_eq!(once.len(), twice.len(), "the same table came back");
    assert!(
        warm < cold / 2 || warm < Duration::from_millis(5),
        "the second read was not a read: cold {cold:?}, warm {warm:?}"
    );
}

/// A table of one's own is never answered out of another's.
///
/// The reason this is a handle and not a static. A cache of the machine's
/// processes shared by the whole process is shared by every test in it: the
/// test that spawns a shell and then sweeps was answered out of a table read
/// before that shell existed, and passed or failed on nothing but which test
/// had last filled the static. CI failed on macOS and passed on Windows, for
/// no reason in either one's own code.
#[test]
fn a_second_table_is_not_answered_out_of_the_first() {
    let filled = Table::default();
    let before = filled.within(SWEEP_FRESH).expect("the process table");

    let mut child = a_child_to_look_for();
    let pid = child.id();
    let read_afresh = Table::default()
        .within(SWEEP_FRESH)
        .expect("a table of its own");

    assert!(
        !before.iter().any(|proc| proc.pid == pid),
        "the child did not exist when the first table was read"
    );
    assert!(
        read_afresh.iter().any(|proc| proc.pid == pid),
        "and a table that has read nothing yet must read now, not borrow that"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// A cached table is genuinely the old one, and asking for no staleness
/// genuinely reads again.
///
/// Observed rather than timed: a child spawned after the cached read cannot be
/// in the cached table and must be in a fresh one. `agent.prompt` acts on this
/// answer, so it calls `snapshot` itself.
#[test]
fn a_stale_table_is_the_old_one_and_a_fresh_one_is_not() {
    use std::time::Duration;

    let table = Table::default();
    // Fill it before the child exists.
    let before = table.within(SWEEP_FRESH).expect("the process table");
    let mut child = a_child_to_look_for();
    let pid = child.id();

    let cached = table.within(SWEEP_FRESH).expect("the cached table");
    let fresh = table.within(Duration::ZERO).expect("a fresh table");

    assert!(
        !before.iter().any(|proc| proc.pid == pid),
        "the child did not exist when the first read happened"
    );
    assert!(
        !cached.iter().any(|proc| proc.pid == pid),
        "so a cached read cannot know about it - which is what cached means"
    );
    assert!(
        fresh.iter().any(|proc| proc.pid == pid),
        "and a read with no staleness allowed must find it"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// No staleness allowed means a read. Pins the contract, whichever part of
/// `already_read` provides it.
#[test]
fn a_request_for_no_staleness_is_never_answered_from_the_cache() {
    use std::time::Duration;

    let table = Table::default();
    // Fill it, so there is something to be wrongly handed back.
    let _ = table.within(SWEEP_FRESH).expect("the process table");

    assert!(
        table.already_read(Duration::ZERO).is_none(),
        "zero means read it again"
    );
    assert!(
        table.already_read(SWEEP_FRESH).is_some(),
        "and a tolerance that was just met is answered from it"
    );
}

/// What the console cannot spell, the script does not let it write.
///
/// A GUI process gets a hidden console, and that console's output code page is
/// the system OEM one - 437 on the owner's machine. PowerShell writes its JSON
/// in that code page, and .NET best-fit maps a bullet onto 0x07 and a curly
/// quote onto a bare `"`, which breaks the document rather than one string of
/// it. Every agent that writes a bullet into a commit message was taking the
/// whole process table down with it, 186 times in ten days (issue #91).
///
/// Run against a console deliberately set to 437, which is what the script has
/// to survive. The marker process carries the characters that did it.
#[cfg(windows)]
#[test]
fn the_table_comes_back_whole_from_a_console_that_cannot_spell() {
    use std::process::{Command, Stdio};

    let hostile = |script: &str| {
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("[Console]::OutputEncoding=[Text.Encoding]::GetEncoding(437);{script}"),
            ])
            .output()
            .expect("powershell")
    };

    // The premise: left to itself, this console cannot write a bullet. Without
    // it the test below would pass on any console and pin nothing.
    let lossy = hostile("[Console]::Write([char]0x2022)");
    assert_ne!(
        lossy.stdout,
        "\u{2022}".as_bytes(),
        "a 437 console wrote a bullet: there is nothing here to survive"
    );

    let mut marker = Command::new("cmd.exe")
        .args(["/c", "pause", "\u{2022}\u{2192}\u{201C}"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("a process whose command line the console cannot spell");
    let pid = marker.id();

    let output = hostile(super::SCRIPT);

    let _ = marker.kill();
    let _ = marker.wait();

    let text = String::from_utf8_lossy(&output.stdout);
    let procs = parse(&text).expect("the whole table, not an error about a control character");
    let found = procs
        .iter()
        .find(|proc| proc.pid == pid)
        .expect("the marker process is in it");
    assert!(
        found.command.contains('\u{2022}') && found.command.contains('\u{201C}'),
        "its command line came through as written: {}",
        found.command
    );
}
