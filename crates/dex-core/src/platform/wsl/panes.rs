//! What a distro can say about its own processes, and what Dex asks it to do
//! about them.
//!
//! Windows' process table cannot see inside WSL: a WSL pane's own process is
//! `wsl.exe`, and everything the pane runs lives in the distro. So each of
//! these is a short `sh` script run through `wsl.exe --exec`, matching
//! processes by the `DEX_PANE_ID` they were started with - the one thing that
//! crosses into Linux with a pane (`wslenv`).
//!
//! Split out of `wsl.rs`, which was at 360 lines.

use std::collections::HashMap;

use super::{quiet, run};

/// Lists, for every process in the distro whose command line names Claude
/// Code, the `DEX_PANE_ID` it was started with - so which Dex panes run
/// Claude Code there, which Windows' process table cannot see. Itself left
/// out: its own command line says "claude" too.
const CLAUDE_PANES: &str = r#"for p in /proc/[0-9]*; do
  [ "$p" = "/proc/$$" ] && continue
  case "$(tr '\0' ' ' 2>/dev/null < "$p/cmdline")" in
    *claude*) tr '\0' '\n' 2>/dev/null < "$p/environ" | sed -n 's/^DEX_PANE_ID=//p' ;;
  esac
done"#;

/// Prints the `DEX_PANE_ID` of every process in the distro that has one, once
/// per process: how many processes each Dex pane runs there. Itself left out.
const PANE_PROCESSES: &str = r#"for p in /proc/[0-9]*; do
  [ "$p" = "/proc/$$" ] && continue
  tr '\0' '\n' 2>/dev/null < "$p/environ" | sed -n 's/^DEX_PANE_ID=//p'
done"#;

/// How many processes each pane runs, from what `PANE_PROCESSES` printed.
pub fn parse_counts(printed: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for pane in printed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        *counts.entry(pane.to_owned()).or_insert(0) += 1;
    }
    counts
}

/// How many processes each Dex pane in `distro` runs. A pane whose count is
/// its shell alone is a shell at its prompt. Blocking.
pub fn pane_processes(distro: &str) -> Result<HashMap<String, usize>, String> {
    let out = run(&["-d", distro, "--exec", "sh", "-c", PANE_PROCESSES])
        .ok_or_else(|| format!("{distro} did not answer"))?;
    if out.status.success() {
        Ok(parse_counts(&String::from_utf8_lossy(&out.stdout)))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// The pane ids `CLAUDE_PANES` printed, once each.
pub fn parse_panes(printed: &str) -> Vec<String> {
    let mut panes: Vec<String> = printed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    panes.sort();
    panes.dedup();
    panes
}

/// The Dex panes in `distro` that run Claude Code now. An error when the
/// distro could not be asked, which is no evidence of anything. Blocking.
///
/// A distro that has stopped answering is not asked at all until its wait is
/// over (`quiet`): the watchdog sweeps every fifteen seconds and the limit on
/// one probe is fifteen seconds, so without that the sweep would spend its
/// whole life inside this call and everything after it in the sweep would wait
/// too (issue #73).
pub fn claude_panes(distro: &str) -> Result<Vec<String>, String> {
    let now = crate::platform::clock::now_millis();
    if quiet::is_resting(distro, now) {
        return Err(format!("{distro} is not answering; not asking again yet"));
    }
    let asked = run(&["-d", distro, "--exec", "sh", "-c", CLAUDE_PANES]);
    let Some(out) = asked else {
        let wait = quiet::went_quiet(distro, now);
        tracing::warn!(%distro, wait_ms = wait, "the distro did not answer; leaving it alone");
        return Err(format!("{distro} did not answer"));
    };
    quiet::answered(distro);
    if out.status.success() {
        Ok(parse_panes(&String::from_utf8_lossy(&out.stdout)))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// How long a closed pane's processes get to go after the hang-up, in tenths
/// of a second: `platform::pty::reap`'s three seconds, counted by the script.
const CLOSE_GRACE_TENTHS: u32 = 30;

/// Hangs up every process carrying `DEX_PANE_ID=$1`, waits for them, and
/// kills whoever is left - `platform::pty::reap` for a distro, which has no
/// process table Windows can read and no job object to put them in.
///
/// By the pane's id rather than by the process tree, which is what reaches
/// the processes that matter: the one that outlived its pane in issue #68 had
/// been reparented to init, so no tree from the pane's shell would have found
/// it. Something that deliberately left its pane behind - `setsid`, a dev
/// server the owner meant to keep - loses the id with its environment only if
/// it was started afresh; a process that still carries the id is still the
/// pane's.
///
/// Prints the pids it hung up, then the pids it had to kill, one line each.
/// An empty `$1` ends nothing: without the guard, the test would match every
/// process in the distro that has no `DEX_PANE_ID` at all.
const END_PANE: &str = r#"pane=$1
[ -n "$pane" ] || exit 0
mine=
for p in /proc/[0-9]*; do
  [ "$p" = "/proc/$$" ] && continue
  id=$(tr '\0' '\n' 2>/dev/null < "$p/environ" | sed -n 's/^DEX_PANE_ID=//p')
  [ "$id" = "$pane" ] && mine="$mine ${p#/proc/}"
done
echo "$mine"
[ -n "$mine" ] || { echo; exit 0; }
kill -HUP $mine 2>/dev/null
left=$mine
i=0
while [ $i -lt GRACE ] && [ -n "$left" ]; do
  sleep 0.1
  still=
  for pid in $left; do [ -e "/proc/$pid" ] && still="$still $pid"; done
  left=$still
  i=$((i + 1))
done
echo "$left"
[ -n "$left" ] && kill -KILL $left 2>/dev/null
exit 0"#;

/// What a distro did about a closed pane's processes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ended {
    /// The processes that carried the pane's id and were hung up.
    pub hung_up: Vec<u32>,
    /// Those of them still running after the grace, which were killed.
    pub killed: Vec<u32>,
}

/// The two lines `END_PANE` prints. A line of anything but pids is read as no
/// pids rather than as an error: the worst a misread costs is a log line.
pub fn parse_ended(printed: &str) -> Ended {
    let pids = |line: Option<&str>| {
        line.unwrap_or_default()
            .split_whitespace()
            .filter_map(|word| word.parse().ok())
            .collect()
    };
    let mut lines = printed.lines();
    Ended {
        hung_up: pids(lines.next()),
        killed: pids(lines.next()),
    }
}

/// Ends everything a closed pane left running inside `distro` (issue #68).
/// Blocking, for up to the grace plus one `wsl.exe` start.
///
/// Dex kills the Windows side of a WSL pane - `wsl.exe` and the pane's PTY -
/// and the distro keeps its own processes: an agent's command outlived its
/// pane by three minutes and then by hours, and had to be killed by hand.
///
/// A distro that cannot be asked is recorded as not answering, so the
/// watchdog leaves it alone for a while (`quiet`); it is still asked once
/// here, resting or not, because this is one pane being closed and not a
/// sweep, and the processes are only going to go on running.
pub fn end_pane_processes(distro: &str, pane: &str) -> Result<Ended, String> {
    if pane.is_empty() {
        return Err("no pane to end processes for".to_owned());
    }
    let script = END_PANE.replace("GRACE", &CLOSE_GRACE_TENTHS.to_string());
    let out = run(&["-d", distro, "--exec", "sh", "-c", &script, "sh", pane]);
    let Some(out) = out else {
        quiet::went_quiet(distro, crate::platform::clock::now_millis());
        return Err(format!("{distro} did not answer"));
    };
    quiet::answered(distro);
    if out.status.success() {
        Ok(parse_ended(&String::from_utf8_lossy(&out.stdout)))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
