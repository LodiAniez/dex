//! `dex doctor`: is Dex running, reachable, and set up? Checks what can be
//! seen from outside the app, and says how to fix each failure (docs/prd.md §11).

use std::process::Command;

use dex_protocol::PROTOCOL_VERSION;
use serde::Serialize;

use crate::commands::{hooks, mcp, skill};
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
    name: &'static str,
    status: Status,
    detail: String,
}

fn check(name: &'static str, status: Status, detail: impl Into<String>) -> Check {
    Check {
        name,
        status,
        detail: detail.into(),
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
                vec![c.name.to_owned(), status.to_owned(), c.detail.clone()]
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
                        "running; pipe {} answered and proved it is Dex",
                        client::pipe_name()
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
            "git.exe is not on PATH; install Git for Windows (worktrees need it)",
        ),
    }
}
