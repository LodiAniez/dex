//! `dex doctor`: is Dex running, reachable, and set up? Checks what can be
//! seen from outside the app, and says how to fix each failure (docs/prd.md §11).

use std::process::Command;

use dex_protocol::PROTOCOL_VERSION;
use serde::Serialize;

use crate::client;
use crate::output::{self, Format};

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

/// Runs every check and prints the results. False if any check failed.
pub fn run(format: Format) -> bool {
    let mut checks = connection_checks();
    checks.push(git());
    checks.push(check(
        "hooks",
        Status::Skip,
        "Claude Code hooks arrive in milestone M5",
    ));
    checks.push(check(
        "mcp",
        Status::Skip,
        "the MCP server arrives in milestone M6",
    ));

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
