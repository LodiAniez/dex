//! `dex mcp install | uninstall | status`: registering `dex-mcp` with Claude
//! Code at user scope (docs/prd.md §10.2).
//!
//! Always through `claude mcp`, never by editing `~/.claude.json`: that file
//! also holds the owner's OAuth session and per-project trust state, and a
//! hand-edit that goes wrong costs them more than this feature is worth. If
//! `claude` is not on PATH this fails and says so.
//!
//! The env values are `${VAR:-}` placeholders Claude Code expands per session.
//! The PRD warns that PowerShell would expand them in the installer's own
//! environment and bake in empty strings; running `claude` directly with an
//! argument list, as here, has no shell to do that. Same reasoning as exec-form
//! hooks.

use std::path::PathBuf;
use std::process::Command;

use clap::Subcommand;
use dex_protocol::{ErrorBody, ErrorCode};

use crate::output::{self, Format};

/// The server name registered with Claude Code.
const SERVER: &str = "dex";

/// The variables `dex-mcp` reads to find its pane, and the placeholders that
/// carry them. `${VAR:-}` yields an empty string when unset; plain `${VAR}`
/// would be passed through as literal text with a missing-variable warning.
const PASSED_THROUGH: [&str; 4] = [
    "DEX_PANE_ID",
    "DEX_WORKSPACE_ID",
    "DEX_AGENT_ID",
    "DEX_SOCKET",
];

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Register the Dex MCP server with Claude Code, for all your projects.
    Install,
    /// Remove the registration.
    Uninstall,
    /// Show whether it is registered, where, and whether it points here.
    Status,
}

pub fn run(command: McpCommand, format: Format) -> Result<(), ErrorBody> {
    match command {
        McpCommand::Status => {
            let found = registration(At::Windows)?;
            if format.json {
                output::json(&describe(&found));
            } else {
                println!("{}", summary(&found));
            }
        }
        McpCommand::Install => install(format)?,
        McpCommand::Uninstall => {
            match registration(At::Windows)? {
                None => println!("The Dex MCP server was not registered."),
                Some(_) => {
                    claude(At::Windows, &["mcp", "remove", SERVER, "-s", "user"])?;
                    println!("Removed the Dex MCP server from your Claude Code config.");
                }
            };
        }
    }
    Ok(())
}

/// Which Claude Code to talk to: the Windows one, or the one in a distro.
#[derive(Debug, Clone, Copy)]
pub(crate) enum At<'a> {
    Windows,
    Wsl(&'a str),
}

fn install(format: Format) -> Result<(), ErrorBody> {
    let server = server_path()?.to_string_lossy().into_owned();
    let Some(after) = install_in(At::Windows, &server)? else {
        println!("Already registered, pointing at this build. Nothing to do.");
        return Ok(());
    };
    if format.json {
        output::json(&describe(&Some(after)));
    } else {
        println!("Registered the Dex MCP server for all your projects.");
        println!("Its tools appear only in Claude Code sessions running inside a Dex pane.");
    }
    Ok(())
}

/// Registers `server` with the Claude Code `at`, at user scope. `None` when it
/// already was, pointing there. Also how `dex wsl setup` registers it inside a
/// distro, with `server` as Linux sees `dex-mcp.exe`.
pub(crate) fn install_in(at: At, server: &str) -> Result<Option<Found>, ErrorBody> {
    let exe = server;
    match registration(at)? {
        // Someone registered `dex` closer than user scope, where it wins. Say
        // so rather than adding a second one that never takes effect.
        Some(found) if !found.user_scope => {
            return Err(ErrorBody {
                code: ErrorCode::InvalidArgs,
                message: format!(
                    "a different `{SERVER}` MCP server is registered at {}",
                    found.scope
                ),
                repair: format!(
                    "That one takes precedence over user scope. Remove it with \
                     `claude mcp remove {SERVER}` in the project it belongs to, then run this again."
                ),
            });
        }
        Some(found) if found.command == exe => return Ok(None),
        // Registered, but pointing at a dex-mcp.exe that has since moved.
        Some(_) => claude(at, &["mcp", "remove", SERVER, "-s", "user"]).map(|_| ())?,
        None => {}
    }

    // Order matters: `-e` is variadic, so a server name placed after the last
    // `-e` is swallowed as another environment value and the command becomes
    // the name. Name first, then the variables, then `--` and the executable.
    let mut args: Vec<String> = vec![
        "mcp".into(),
        "add".into(),
        "--scope".into(),
        "user".into(),
        SERVER.into(),
    ];
    for name in PASSED_THROUGH {
        args.push("-e".into());
        args.push(format!("{name}=${{{name}:-}}"));
    }
    args.push("--".into());
    args.push(exe.to_owned());
    claude(at, &args.iter().map(String::as_str).collect::<Vec<_>>())?;

    // Read back: a registration that silently lost its placeholders would look
    // installed and do nothing in every session.
    let after = registration(at)?.ok_or_else(|| ErrorBody {
        code: ErrorCode::Internal,
        message: "Claude Code accepted the registration but does not list it".into(),
        repair: "Run `claude mcp list` to see what it has, and report this.".into(),
    })?;
    if !after.placeholders_intact {
        return Err(ErrorBody {
            code: ErrorCode::Internal,
            message: "the registration lost its ${DEX_PANE_ID} placeholder".into(),
            repair: format!(
                "Remove it with `claude mcp remove {SERVER} -s user` and report this: without the \
                 placeholder the server cannot tell which pane it is in."
            ),
        });
    }
    Ok(Some(after))
}

/// What Claude Code has registered under our name, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The scope line, as Claude Code words it.
    pub scope: String,
    /// Whether that scope is user scope, the only one Dex installs to.
    pub user_scope: bool,
    /// The executable it runs.
    pub command: String,
    /// Whether `DEX_PANE_ID` is still passed as a placeholder.
    pub placeholders_intact: bool,
}

pub(crate) fn registration(at: At) -> Result<Option<Found>, ErrorBody> {
    let output = run_claude(at, &["mcp", "get", SERVER])?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(parse(&String::from_utf8_lossy(&output.stdout))))
}

/// Reads `claude mcp get <name>` output.
///
/// Note it renders the env values without their defaults — a stored
/// `${DEX_PANE_ID:-}` prints as `${DEX_PANE_ID}` — so the check is for the
/// placeholder's presence, not its exact text. The stored value is what counts.
fn parse(text: &str) -> Found {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(&format!("{name}: ")))
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    let scope = field("Scope");
    Found {
        user_scope: scope.to_ascii_lowercase().starts_with("user"),
        scope: if scope.is_empty() {
            "an unknown scope".into()
        } else {
            scope
        },
        command: field("Command"),
        placeholders_intact: text.contains("DEX_PANE_ID=${DEX_PANE_ID"),
    }
}

fn summary(found: &Option<Found>) -> String {
    match found {
        None => "Not registered. Run `dex mcp install` to add it.".into(),
        Some(found) if !found.user_scope => format!(
            "Registered at {}, which overrides user scope.\n  Command: {}",
            found.scope, found.command
        ),
        Some(found) => {
            let here = server_path()
                .map(|exe| exe.to_string_lossy() == found.command)
                .unwrap_or(false);
            format!(
                "Registered for all your projects.\n  Command: {}{}",
                found.command,
                if here {
                    ""
                } else {
                    "\n  This is not the dex-mcp.exe beside the dex.exe you just ran; \
                     run `dex mcp install` to point it here."
                }
            )
        }
    }
}

fn describe(found: &Option<Found>) -> serde_json::Value {
    match found {
        None => serde_json::json!({ "registered": false }),
        Some(found) => serde_json::json!({
            "registered": true,
            "scope": found.scope,
            "user_scope": found.user_scope,
            "command": found.command,
            "placeholders_intact": found.placeholders_intact,
        }),
    }
}

/// `dex-mcp.exe`, beside the `dex.exe` that is running.
fn server_path() -> Result<PathBuf, ErrorBody> {
    let exe = std::env::current_exe().map_err(|err| ErrorBody {
        code: ErrorCode::Internal,
        message: format!("cannot find this dex.exe: {err}"),
        repair: "Check the path and its permissions, then run this again.".into(),
    })?;
    let server = exe.with_file_name(format!("dex-mcp{}", std::env::consts::EXE_SUFFIX));
    if !server.exists() {
        return Err(ErrorBody {
            code: ErrorCode::Internal,
            message: format!("{} is missing", server.display()),
            repair: "dex-mcp must sit beside dex; reinstall Dex, or build it with `cargo build --release`."
                .into(),
        });
    }
    Ok(server)
}

fn claude(at: At, args: &[&str]) -> Result<String, ErrorBody> {
    let output = run_claude(at, args)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        return Err(ErrorBody {
            code: ErrorCode::Internal,
            message: format!(
                "`claude {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            repair: match at {
                At::Windows => "Run that command yourself to see what Claude Code reports.".into(),
                At::Wsl(distro) => {
                    format!("Run that command in a {distro} pane to see what Claude Code reports.")
                }
            },
        });
    }
    Ok(stdout)
}

fn run_claude(at: At, args: &[&str]) -> Result<std::process::Output, ErrorBody> {
    match at {
        At::Windows => Command::new("claude").args(args).output(),
        At::Wsl(distro) => crate::wsl::run_login(distro, "claude", args),
    }
    .map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            return ErrorBody {
                code: ErrorCode::Internal,
                message: "`claude` is not on PATH".into(),
                repair:
                    "Install Claude Code, or open a new terminal so PATH includes it. Dex will \
                         not edit ~/.claude.json by hand."
                        .into(),
            };
        }
        ErrorBody {
            code: ErrorCode::Internal,
            message: format!("cannot run `claude`: {err}"),
            repair: "Check that Claude Code is installed and runnable, then try again.".into(),
        }
    })
}

/// For `dex doctor`: one line on the MCP registration.
pub fn doctor_check() -> (bool, String) {
    match registration(At::Windows) {
        Err(err) => (false, err.message),
        Ok(None) => (false, "not registered (run `dex mcp install`)".into()),
        Ok(Some(found)) if !found.user_scope => (
            false,
            format!("registered at {}, not user scope", found.scope),
        ),
        Ok(Some(found)) => {
            let here = server_path()
                .map(|exe| exe.to_string_lossy() == found.command)
                .unwrap_or(false);
            match here {
                true => (true, "registered for all projects".into()),
                false => (
                    false,
                    format!("registered, but points at {}", found.command),
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests;
