use super::*;

fn report(claude: bool, command: bool, hooks: bool, mcp: bool, skill: bool) -> Report {
    Report {
        claude: claude.then(|| "2.1.276 (Claude Code)".to_owned()),
        command,
        hooks,
        mcp,
        skill,
    }
}

#[test]
fn what_is_missing_is_named_in_the_order_setup_installs_it() {
    assert_eq!(
        report(true, false, true, false, false).missing(),
        vec!["dex command", "MCP server", "skill"]
    );
    assert!(report(true, true, true, true, true).missing().is_empty());
}

#[test]
fn the_status_line_says_what_to_do_next() {
    assert_eq!(
        describe("Ubuntu", &report(false, true, true, false, true)),
        "Claude Code is not installed in Ubuntu; install it there to run agents in it"
    );
    assert_eq!(
        describe("Ubuntu", &report(true, true, true, true, true)),
        "set up: agents in Ubuntu report to Dex"
    );
    assert_eq!(
        describe("Ubuntu", &report(true, false, true, true, false)),
        "not set up for Dex: no dex command, no skill (run `dex wsl setup Ubuntu`)"
    );
}
