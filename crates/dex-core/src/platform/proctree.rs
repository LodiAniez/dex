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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// Whether nothing runs under the shell `shell_pid` - a plain shell at its
/// prompt, which can be closed and started again elsewhere without losing
/// anything. `None` if the shell is not in the table. A console host the
/// system starts beside a console program is not the owner's.
pub fn bare(procs: &[Proc], shell_pid: u32) -> Option<bool> {
    procs.iter().find(|proc| proc.pid == shell_pid)?;
    Some(!procs.iter().any(|proc| {
        let name = proc.name.to_ascii_lowercase();
        proc.parent == shell_pid
            && proc.pid != shell_pid
            && !name.starts_with("conhost")
            && !name.starts_with("openconsole")
    }))
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
    // A WSL pane: its own process is `wsl.exe`, and what runs in it is hidden.
    if shell.name.to_ascii_lowercase().starts_with("wsl") {
        return Presence::CannotTell;
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

#[cfg(windows)]
const SCRIPT: &str = "Get-CimInstance Win32_Process | ForEach-Object { [pscustomobject]@{ p = $_.ProcessId; pp = $_.ParentProcessId; n = $_.Name; c = $_.CommandLine } } | ConvertTo-Json -Compress";

/// Reads `ps -A -o pid=,ppid=,args=`. A process's name is its first word
/// without the directory - and without the `-` a login shell is shown with -
/// which is what `claude_under` looks at; lines `ps` could not fill in are skipped.
pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let pid = words.next()?.parse().ok()?;
            let parent = words.next()?.parse().ok()?;
            let command = words.collect::<Vec<_>>().join(" ");
            let first = command.split_whitespace().next()?;
            let name = first
                .rsplit('/')
                .next()
                .unwrap_or(first)
                .trim_start_matches('-');
            Some(Proc {
                pid,
                parent,
                name: name.to_owned(),
                command,
            })
        })
        .collect()
}

/// The process table, now. Blocking: call it off the async runtime.
#[cfg(not(windows))]
pub fn snapshot() -> io::Result<Vec<Proc>> {
    let output = Command::new("ps")
        .args(["-A", "-ww", "-o", "pid=,ppid=,args="])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("the process table could not be read"));
    }
    Ok(parse_ps(&String::from_utf8_lossy(&output.stdout)))
}

/// How stale a table a watchdog sweep will accept.
///
/// **It must stay under `presence::LOOK_AGAIN_AFTER`**, and a test over there
/// says so: a departure is confirmed by looking twice, two seconds apart,
/// because Claude Code replaces its own process when it updates and gone for a
/// moment is not gone. Two looks answered out of one table would confirm
/// nothing, and would end an agent that was only restarting. Inside that bound
/// the two reads a single sweep makes back to back - whose Claude Code has
/// gone, and whether a silent agent is inside a tool call - become one.
///
/// The tolerance lives here rather than in either slice that reads it: the
/// agent slice already calls the workspace slice, so a constant owned by one
/// of them would have the other reaching back (conventions 1.2).
pub const SWEEP_FRESH: Duration = Duration::from_secs(1);

/// A process table, read no more often than its readers need.
///
/// Reading it costs real time - on Windows it is a PowerShell start and a CIM
/// enumeration of every process, measured at **620 ms** on the owner's machine
/// for 370 processes - and one watchdog sweep asked for it twice over (issue
/// #86).
///
/// A handle, held in `AppState` beside the other platform services, rather
/// than a static: a cache of the machine's real processes shared by a whole
/// process is one shared by every test in it, and the test that spawns a shell
/// and then asks about it would be answered out of a table read before the
/// shell existed. CI caught exactly that. Each `AppState` has its own, so a
/// test is answered about its own machine at its own moment.
///
/// Callers say what staleness they can live with; nothing here decides for
/// them. `agent.prompt` calls `snapshot` itself, because it is about to act on
/// the answer.
#[derive(Clone, Default)]
pub struct Table(Arc<Mutex<Option<LastRead>>>);

/// A table and when it was read.
type LastRead = (Instant, Vec<Proc>);

impl Table {
    /// The table, read afresh unless the last read was no longer ago than
    /// `max_age`. Blocking: call it off the async runtime.
    pub fn within(&self, max_age: Duration) -> io::Result<Vec<Proc>> {
        if let Some(procs) = self.already_read(max_age) {
            return Ok(procs);
        }
        let procs = snapshot()?;
        if let Ok(mut held) = self.0.lock() {
            *held = Some((Instant::now(), procs.clone()));
        }
        Ok(procs)
    }

    /// The table already read, if that was recent enough to pass for now.
    ///
    /// `ZERO` returns early rather than being left to the comparison below,
    /// which would also reject it: "read less than no time ago" is false for
    /// any elapsed time above zero, and elapsed time here is never zero in
    /// practice. The early return says what is meant instead of relying on
    /// that, and **no test can tell the two apart** - removing it changes no
    /// observable behaviour, which mutation testing confirmed. It is kept as
    /// the statement of intent, not because anything exercises it.
    fn already_read(&self, max_age: Duration) -> Option<Vec<Proc>> {
        if max_age.is_zero() {
            return None;
        }
        self.0
            .lock()
            .ok()
            .and_then(|held| held.clone())
            .filter(|(read_at, _)| read_at.elapsed() <= max_age)
            .map(|(_, procs)| procs)
    }
}

/// The process table, now. Blocking: call it off the async runtime.
#[cfg(windows)]
pub fn snapshot() -> io::Result<Vec<Proc>> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
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
