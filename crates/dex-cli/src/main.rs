//! `dex`: the command-line client. Thin by design — parse, send requests over
//! the pipe, print the responses. All logic lives in the daemon.
#![forbid(unsafe_code)]

mod commands;
mod output;

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use commands::agent::AgentCommand;
use commands::config::ConfigCommand;
use commands::context::ContextCommand;
use commands::hooks::HooksCommand;
use commands::mcp::McpCommand;
use commands::pane::PaneCommand;
use commands::repo::{RepoCommand, WorktreeCommand};
use commands::skill::SkillCommand;
use commands::workspace::WorkspaceCommand;
use output::Format;

/// Command-line interface to the running Dex app.
#[derive(Debug, Parser)]
#[command(
    name = "dex",
    version,
    about = "Terminal workspace multiplexer for multi-agent coding",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    /// Leave out the header row of tables.
    #[arg(long, global = true)]
    no_header: bool,
    /// Workspace to act on, when not running inside a Dex pane.
    #[arg(long, global = true)]
    workspace: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check that Dex is running and reachable, and what is set up.
    Doctor,
    /// List, create, switch, and delete workspaces.
    #[command(subcommand)]
    Workspace(WorkspaceCommand),
    /// List, create, split, and close panes; type into them; label them.
    #[command(subcommand)]
    Pane(PaneCommand),
    /// List and stop the Claude Code sessions running in panes.
    #[command(subcommand)]
    Agent(AgentCommand),
    /// Register and inspect git repositories.
    #[command(subcommand)]
    Repo(RepoCommand),
    /// Create and remove the git worktrees agents work in.
    #[command(subcommand)]
    Worktree(WorktreeCommand),
    /// Read and write the workspace's shared context store.
    #[command(subcommand)]
    Context(ContextCommand),
    /// Install, remove, or check Dex's Claude Code hooks.
    #[command(subcommand)]
    Hooks(HooksCommand),
    /// Register or remove the Dex MCP server with Claude Code.
    #[command(subcommand)]
    Mcp(McpCommand),
    /// Show where Dex's settings live and what it is running with.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Install, remove, or check the Dex skill for Claude Code agents.
    #[command(subcommand)]
    Skill(SkillCommand),
    /// Claude Code hook entry point. Always exits 0; a no-op outside a Dex pane.
    Event(EventArgs),
}

#[derive(Debug, Args)]
struct EventArgs {
    /// Which hook fired (session-start, prompt, batch, stop, ...).
    // A plain string rather than an enum on purpose: an unknown value must not
    // make clap exit non-zero, because a failing hook interrupts the agent.
    kind: String,
}

fn main() -> ExitCode {
    // Hooks are user-level, so `dex event` runs in every Claude Code session on
    // the machine, including ones unrelated to Dex. Outside a Dex pane it must
    // cost nothing beyond process startup: no argument parsing, no pipe, no
    // output. This check therefore comes before everything else (PRD §9.3).
    if is_hook_outside_dex() {
        return ExitCode::SUCCESS;
    }

    let cli = Cli::parse();
    let format = Format {
        json: cli.json,
        header: !cli.no_header,
    };
    let result = match cli.command {
        Command::Event(args) => {
            commands::event::run(&args.kind);
            return ExitCode::SUCCESS;
        }
        Command::Doctor => {
            return if commands::doctor::run(format) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        Command::Workspace(command) => commands::workspace::run(command, format),
        Command::Pane(command) => commands::pane::run(command, format),
        Command::Agent(command) => commands::agent::run(command, format, cli.workspace),
        Command::Repo(command) => commands::repo::run(command, format),
        Command::Worktree(command) => commands::repo::run_worktree(command, format),
        Command::Context(command) => commands::context::run(command, format, cli.workspace),
        Command::Hooks(command) => commands::hooks::run(command, format),
        Command::Mcp(command) => commands::mcp::run(command, format),
        Command::Config(command) => commands::config::run(command, format),
        Command::Skill(command) => commands::skill::run(command, format),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            output::print_error(&err, format);
            ExitCode::FAILURE
        }
    }
}

/// True for `dex event ...` when not running inside a Dex pane.
fn is_hook_outside_dex() -> bool {
    let is_event = std::env::args_os().nth(1).is_some_and(|arg| arg == "event");
    let has_pane = std::env::var_os("DEX_PANE_ID").is_some_and(|value| !value.is_empty());
    is_event && !has_pane
}
