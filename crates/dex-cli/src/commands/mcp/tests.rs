use super::*;

/// Real `claude mcp get dex` output, captured from 2.1.273.
const USER_SCOPE: &str = "\
dex:
  Scope: User config (available in all your projects)
  Status: \u{221a} Connected
  Type: stdio
  Command: C:\\Users\\Admin\\Desktop\\projects\\dex\\target\\release\\dex-mcp.exe
  Args:
  Environment:
    DEX_PANE_ID=${DEX_PANE_ID}
    DEX_SOCKET=${DEX_SOCKET}

To remove this server, run: claude mcp remove dex -s user
";

#[test]
fn a_user_scope_registration_is_read_in_full() {
    let found = parse(USER_SCOPE);
    assert!(found.user_scope);
    assert!(found.scope.starts_with("User config"));
    assert_eq!(
        found.command,
        "C:\\Users\\Admin\\Desktop\\projects\\dex\\target\\release\\dex-mcp.exe"
    );
}

#[test]
fn the_placeholder_counts_as_intact_even_though_get_hides_its_default() {
    // Claude Code stores `${DEX_PANE_ID:-}` but prints `${DEX_PANE_ID}`, so a
    // check for the exact stored text would fail on a correct registration.
    assert!(parse(USER_SCOPE).placeholders_intact);
    assert!(parse("  Environment:\n    DEX_PANE_ID=${DEX_PANE_ID:-}\n").placeholders_intact);
}

#[test]
fn a_registration_that_baked_in_an_empty_value_is_not_intact() {
    // What PowerShell double quotes would have produced: expanded at install
    // time, in an environment where the variable was unset.
    let baked = "  Scope: User config\n  Environment:\n    DEX_PANE_ID=\n";
    assert!(!parse(baked).placeholders_intact);
}

#[test]
fn a_project_scope_registration_is_not_user_scope() {
    let project = "  Scope: Project config (shared via .mcp.json)\n  Command: other.exe\n";
    let found = parse(project);
    assert!(!found.user_scope);
    assert_eq!(found.command, "other.exe");
}

#[test]
fn a_local_scope_registration_is_not_user_scope() {
    let local = "  Scope: Local config (private to you in this project)\n  Command: x.exe\n";
    assert!(!parse(local).user_scope);
}

#[test]
fn output_that_makes_no_sense_does_not_panic_or_claim_user_scope() {
    let found = parse("something unexpected entirely");
    assert!(!found.user_scope);
    assert_eq!(found.scope, "an unknown scope");
    assert_eq!(found.command, "");
    assert!(!found.placeholders_intact);
}

#[test]
fn every_variable_dex_mcp_reads_is_passed_through() {
    // dex-mcp gates on DEX_PANE_ID and finds the daemon through DEX_SOCKET;
    // dropping one would make it silently useless.
    assert!(PASSED_THROUGH.contains(&"DEX_PANE_ID"));
    assert!(PASSED_THROUGH.contains(&"DEX_SOCKET"));
    assert!(PASSED_THROUGH.contains(&"DEX_AGENT_ID"));
    assert!(PASSED_THROUGH.contains(&"DEX_WORKSPACE_ID"));
}

#[test]
fn the_summary_says_what_to_do_in_each_state() {
    assert!(summary(&None).contains("dex mcp install"));

    let elsewhere = Found {
        scope: "Local config".into(),
        user_scope: false,
        command: "x.exe".into(),
        placeholders_intact: true,
    };
    assert!(summary(&Some(elsewhere)).contains("overrides user scope"));
}
