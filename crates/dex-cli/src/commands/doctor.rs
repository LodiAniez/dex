//! `dex doctor`: is Dex running, reachable, and set up? Checks what can be
//! seen from outside the app, and says how to fix each failure (docs/prd.md §11).

use std::process::Command;

use dex_protocol::PROTOCOL_VERSION;
use serde::Serialize;

use dex_protocol::pane::{PaneList, TerminalView};

use crate::commands::{hooks, mcp, skill, wsl};
use crate::output::{self, Format};
use dex_cli::client;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Status {
    Ok,
    Fail,
    Skip,
}

#[derive(Debug, Serialize)]
struct Check {
    name: String,
    status: Status,
    detail: String,
    /// The `dex` command that fixes this, for the setup panel's button, when
    /// it takes arguments (`wsl setup <distro>`). The fixed steps it knows.
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<String>,
}

fn check(name: impl Into<String>, status: Status, detail: impl Into<String>) -> Check {
    Check {
        name: name.into(),
        status,
        detail: detail.into(),
        fix: None,
    }
}

/// A check that reports its own pass and detail.
fn verdict(name: &'static str, (ok, detail): (bool, String)) -> Check {
    check(name, if ok { Status::Ok } else { Status::Fail }, detail)
}

/// Runs every check and prints the results. False if any check failed.
pub fn run(format: Format) -> bool {
    let mut checks = connection_checks();
    checks.push(claude());
    checks.push(git());
    checks.push(verdict("hooks", hooks::doctor_check()));
    checks.push(verdict("mcp", mcp::doctor_check()));
    checks.push(verdict("skill", skill::doctor_check()));
    for (name, passed, detail, fixable) in wsl::doctor_checks(&distros_in_use()) {
        let status = match passed {
            Some(true) => Status::Ok,
            Some(false) => Status::Fail,
            None => Status::Skip,
        };
        let distro = name.trim_start_matches("wsl:").to_owned();
        let mut line = check(name, status, detail);
        if passed != Some(true) && fixable {
            line.fix = Some(format!("wsl setup {distro}"));
        }
        checks.push(line);
    }

    let healthy = checks.iter().all(|c| c.status != Status::Fail);
    if format.json {
        output::json(&serde_json::json!({ "ok": healthy, "checks": checks }));
    } else {
        let rows: Vec<Vec<String>> = checks
            .iter()
            .map(|c| {
                let status = match c.status {
                    Status::Ok => "ok",
                    Status::Fail => "FAIL",
                    Status::Skip => "skip",
                };
                vec![c.name.clone(), status.to_owned(), c.detail.clone()]
            })
            .collect();
        output::table(format, &["CHECK", "STATUS", "DETAIL"], &rows);
    }
    healthy
}

/// The app is running, answers on the pipe, proves it is Dex, and speaks our version.
fn connection_checks() -> Vec<Check> {
    match client::connect() {
        Ok(client) => {
            let version = client.app_version();
            let version_check = if version == PROTOCOL_VERSION {
                check(
                    "version",
                    Status::Ok,
                    format!("app and CLI are both {version}"),
                )
            } else {
                check(
                    "version",
                    Status::Fail,
                    format!(
                        "app is {version}, CLI is {PROTOCOL_VERSION}: install both from the same build"
                    ),
                )
            };
            vec![
                check(
                    "app",
                    Status::Ok,
                    format!(
                        "running; {} answered and proved it is Dex",
                        client::describe_address()
                    ),
                ),
                version_check,
            ]
        }
        Err(err) => vec![
            check(
                "app",
                Status::Fail,
                format!("{}. {}", err.message, err.repair),
            ),
            check("version", Status::Skip, "needs the app running"),
        ],
    }
}

/// The distros Dex's panes run in, and the one chosen as its terminal, as far
/// as the app says; none if it is not running.
fn distros_in_use() -> Vec<String> {
    let Ok(mut client) = client::connect() else {
        return Vec::new();
    };
    let chosen = client
        .call::<TerminalView>("pane.terminal", serde_json::json!({}))
        .map(|view| view.terminal)
        .ok();
    let running = client
        .call::<PaneList>("pane.list", serde_json::json!({}))
        .map(|list| {
            list.panes
                .into_iter()
                .map(|pane| pane.runtime)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut distros: Vec<String> = running
        .into_iter()
        .chain(chosen)
        .filter_map(|runtime| runtime.strip_prefix("wsl:").map(str::to_owned))
        .collect();
    distros.sort();
    distros.dedup();
    distros
}

/// Claude Code itself. Without it the hooks and the MCP server have nothing
/// to attach to, so this comes before either of those checks.
fn claude() -> Check {
    match Command::new("claude").arg("--version").output() {
        Ok(out) if out.status.success() => check(
            "claude",
            Status::Ok,
            format!(
                "Claude Code {}",
                String::from_utf8_lossy(&out.stdout).trim()
            ),
        ),
        _ => check(
            "claude",
            Status::Fail,
            "claude is not on PATH; install Claude Code (https://claude.com/claude-code), then open a new terminal",
        ),
    }
}

fn git() -> Check {
    match Command::new("git").arg("--version").output() {
        Ok(out) if out.status.success() => check(
            "git",
            Status::Ok,
            String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        ),
        _ => check(
            "git",
            Status::Fail,
            if cfg!(windows) {
                "git.exe is not on PATH; install Git for Windows (worktrees need it)"
            } else {
                "git is not on PATH; install the Xcode Command Line Tools (`xcode-select --install`) or Git from Homebrew (worktrees need it)"
            },
        ),
    }
}
