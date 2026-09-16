//! `dex config path | show | reload`: the settings file (docs/prd.md §13).
//!
//! `path` answers without the daemon, because the most common reason to ask is
//! that Dex will not start and the owner wants to look at the file. The other
//! two go through the daemon, which is the only thing that knows what is
//! actually in force — the file on disk may have been edited since.

use std::path::PathBuf;

use clap::Subcommand;
use dex_protocol::ErrorBody;
use dex_protocol::config::ConfigView;
use serde_json::json;

use crate::output::{self, Format};
use dex_cli::client;

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print where the settings file is.
    Path,
    /// Print the settings Dex is running with, defaults filled in.
    Show,
    /// Re-read the file now, and report anything wrong with it.
    Reload,
}

pub fn run(command: ConfigCommand, format: Format) -> Result<(), ErrorBody> {
    if let ConfigCommand::Path = command {
        let path = local_path();
        if format.json {
            output::json(&json!({ "path": path }));
        } else {
            println!("{path}");
        }
        return Ok(());
    }

    let mut client = client::connect()?;
    let view: ConfigView = match command {
        ConfigCommand::Reload => client.call("config.reload", json!({}))?,
        _ => client.call("config.get", json!({}))?,
    };
    if format.json {
        output::json(&view);
        return Ok(());
    }
    println!("# {}", view.path);
    if !view.present {
        println!("# (no file yet — these are the defaults)");
    }
    print!("{}", view.effective);
    for problem in &view.problems {
        eprintln!("warning: {problem}");
    }
    Ok(())
}

/// `%APPDATA%\Dex\config.toml`, worked out without asking the daemon.
fn local_path() -> String {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default();
    base.join("Dex")
        .join("config.toml")
        .to_string_lossy()
        .replace('\\', "/")
}
