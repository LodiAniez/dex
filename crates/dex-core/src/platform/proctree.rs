//! Whether Claude Code is running under a pane's shell.
//!
//! An agent's status is what its hooks last said, and Claude Code does not
//! always get to say goodbye: quit with Ctrl+C, or crashed, it fires no
//! `SessionEnd`, and the agent goes on reading "idle" over what is now a bare
//! shell. Anything typed there, the shell runs. The process table is the only
//! thing that knows: Dex looks before it types at an agent, and the watchdog
//! looks every sweep and ends whoever is not there (`agent/presence.rs`).
//!
//! The table is read by asking PowerShell, which every supported Windows has;
//! `#![forbid(unsafe_code)]` rules out the Toolhelp API. It takes a few hundred
//! milliseconds, so the watchdog reads it only when there is an agent to ask
//! about.

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::io;
use std::process::Command;

use serde::Deserialize;

/// One row of the process table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    /// Image name, e.g. `claude.exe`.
    pub name: String,
    /// Command line, or empty when it could not be read.
    pub command: String,
}

/// What the process table says about a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// A Claude Code process runs under the pane's shell.
    There,
    /// The shell is there and nothing under it is Claude Code.
    Gone,
    /// The table cannot say: the shell is not in it, what runs in the pane runs
    /// inside WSL, which this table does not see into, or a runtime is there
    /// whose command line could not be read.
    CannotTell,
}

/// Runtimes Claude Code may run under, named only on the command line.
const RUNTIMES: [&str; 3] = ["node", "bun", "deno"];

/// Generous on purpose: the watchdog ends whoever is judged gone, so a miss
/// costs an agent, while a false match only keeps a dead one a little longer.
fn is_claude(proc: &Proc) -> bool {
    // The native build is `claude.exe`; an npm, bun or deno install runs as its
    // runtime, named by the package on its command line.
    proc.name.to_ascii_lowercase().starts_with("claude")
        || proc.command.to_ascii_lowercase().contains("claude")
}

/// A runtime whose command line could not be read: it may be Claude Code.
fn is_unreadable_runtime(proc: &Proc) -> bool {
    let name = proc.name.to_ascii_lowercase();
    proc.command.is_empty() && RUNTIMES.iter().any(|runtime| name.starts_with(runtime))
}

/// Whether Claude Code runs under `shell_pid`, at any depth.
pub fn claude_under(procs: &[Proc], shell_pid: u32) -> Presence {
    let Some(shell) = procs.iter().find(|proc| proc.pid == shell_pid) else {
        return Presence::CannotTell;
    };
    // `shell = "claude"`: the pane's own process is it.
    if shell.name.to_ascii_lowercase().starts_with("claude") {
        return Presence::There;
    }
    let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for proc in procs {
        children.entry(proc.parent).or_default().push(proc);
    }
    let mut seen = HashSet::from([shell_pid]);
    let mut queue = vec![shell_pid];
    let mut unseen = false;
    while let Some(pid) = queue.pop() {
        for child in children.get(&pid).into_iter().flatten() {
            // Pids are reused, and a corrupt table can loop.
            if !seen.insert(child.pid) {
                continue;
            }
            if is_claude(child) {
                return Presence::There;
            }
            // WSL cannot be seen into, and an unreadable runtime cannot be told apart.
            unseen |=
                child.name.to_ascii_lowercase().starts_with("wsl") || is_unreadable_runtime(child);
            queue.push(child.pid);
        }
    }
    if unseen {
        Presence::CannotTell
    } else {
        Presence::Gone
    }
}

#[derive(Deserialize)]
struct Row {
    p: u32,
    pp: u32,
    n: Option<String>,
    c: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Rows {
    Many(Vec<Row>),
    // PowerShell prints a lone object, not a list of one.
    One(Row),
}

/// Parses what `snapshot` asks PowerShell to print.
pub fn parse(json: &str) -> Result<Vec<Proc>, serde_json::Error> {
    let rows = match serde_json::from_str::<Rows>(json.trim())? {
        Rows::Many(rows) => rows,
        Rows::One(row) => vec![row],
    };
    Ok(rows
        .into_iter()
        .map(|row| Proc {
            pid: row.p,
            parent: row.pp,
            name: row.n.unwrap_or_default(),
            command: row.c.unwrap_or_default(),
        })
        .collect())
}

const SCRIPT: &str = "Get-CimInstance Win32_Process | ForEach-Object { [pscustomobject]@{ p = $_.ProcessId; pp = $_.ParentProcessId; n = $_.Name; c = $_.CommandLine } } | ConvertTo-Json -Compress";

/// The process table, now. Blocking: call it off the async runtime.
pub fn snapshot() -> io::Result<Vec<Proc>> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up over the owner's work.
        command.creation_flags(0x0800_0000);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(io::Error::other("the process table could not be read"));
    }
    parse(&String::from_utf8_lossy(&output.stdout)).map_err(io::Error::other)
}
