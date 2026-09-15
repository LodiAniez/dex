//! `dex`: the command-line client. Thin by design — parse, send one request
//! over the pipe, print the response. All logic lives in the daemon.
#![forbid(unsafe_code)]

mod commands;

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

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
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Claude Code hook entry point. Always exits 0; a no-op outside a Dex pane.
    Event(EventArgs),
    /// Measures one pipe round trip (scripts/bench-hooks.ps1). Replaced by the real client in M4.
    #[command(hide = true)]
    BenchPing {
        /// Pipe name, without the `\\.\pipe\` prefix.
        #[arg(long)]
        pipe: String,
    },
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
    match cli.command {
        Command::Event(args) => {
            commands::event::run(&args.kind);
            ExitCode::SUCCESS
        }
        Command::BenchPing { pipe } => match commands::bench::ping(&pipe) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("dex: {err:#}");
                ExitCode::FAILURE
            }
        },
    }
}

/// True for `dex event ...` when not running inside a Dex pane.
fn is_hook_outside_dex() -> bool {
    let is_event = std::env::args_os().nth(1).is_some_and(|arg| arg == "event");
    let has_pane = std::env::var_os("DEX_PANE_ID").is_some_and(|value| !value.is_empty());
    is_event && !has_pane
}
