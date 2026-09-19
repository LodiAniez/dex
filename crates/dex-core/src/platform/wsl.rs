//! Running things inside WSL: where a pane or an agent runs, when it is not
//! Windows (a pane's `runtime`).
//!
//! Nothing here needs a daemon inside the distro. Programs in WSL run Windows
//! programs directly (interop), so an agent there uses the `dex.exe` that is
//! already installed, over the same named pipe as everyone else. What has to
//! cross into Linux is the pane's identity: `WSLENV` names the variables
//! `wsl.exe` passes in, and passes back out when Linux runs a Windows program.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Where a pane's processes run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runtime {
    /// On Windows itself.
    Windows,
    /// Inside the named WSL distro.
    Wsl(String),
}

impl Runtime {
    /// Reads a stored runtime: `windows` or `wsl:<distro>`.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text == "windows" {
            return Ok(Self::Windows);
        }
        let Some(distro) = text.strip_prefix("wsl:") else {
            return Err(format!(
                "{text:?} is not a runtime; expected \"windows\" or \"wsl:<distro>\""
            ));
        };
        let named = !distro.is_empty()
            && !distro.starts_with('-')
            && distro
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if named {
            Ok(Self::Wsl(distro.to_owned()))
        } else {
            Err(format!("{distro:?} is not the name of a WSL distro"))
        }
    }
}

impl fmt::Display for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Windows => f.write_str("windows"),
            Self::Wsl(distro) => write!(f, "wsl:{distro}"),
        }
    }
}

/// The variables a pane in WSL must see, and a Windows program it runs (such as
/// `dex.exe` from a hook) must see again. `DEX_DATA_DIR` too, when the app has
/// one: `dex.exe` finds the app's token there, and a Windows program started
/// from Linux does not get the app's own environment.
pub const PASSED_IN: [&str; 5] = [
    "DEX_PANE_ID",
    "DEX_WORKSPACE_ID",
    "DEX_AGENT_ID",
    "DEX_SOCKET",
    "DEX_DATA_DIR",
];

/// `WSLENV` with `names` added to what the owner already has. No flag: a
/// variable goes both ways, into Linux and back out to Windows programs.
pub fn wslenv(existing: Option<&str>, names: &[&str]) -> String {
    let mut entries: Vec<String> = existing
        .unwrap_or_default()
        .split(':')
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect();
    for name in names {
        let listed = entries
            .iter()
            .any(|entry| entry.split('/').next() == Some(*name));
        if !listed {
            entries.push((*name).to_owned());
        }
    }
    entries.join(":")
}

/// What `wsl.exe` is given to start a pane: the distro's login shell, in
/// `cwd` (a Windows path, which `wsl.exe` translates). Given with backslashes:
/// `wsl.exe` reads `C:/src` either way, but `//wsl.localhost/...` - a folder
/// inside the Linux filesystem - it silently replaces with `/`.
pub fn pane_args(distro: &str, cwd: &str) -> Vec<String> {
    let cwd = cwd.replace('/', "\\");
    ["-d", distro, "--cd", &cwd].map(str::to_owned).to_vec()
}

/// Distros that are some other program's machinery, not a place to work.
const NOT_FOR_AGENTS: [&str; 3] = ["docker-desktop", "rancher-desktop", "podman-machine"];

/// The distros `wsl.exe -l -q` printed, less other programs' own.
pub fn parse_distros(printed: &[u8]) -> Vec<String> {
    let units: Vec<u16> = printed
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    String::from_utf16_lossy(&units)
        .lines()
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\0'))
        .filter(|name| {
            !name.is_empty() && !NOT_FOR_AGENTS.iter().any(|other| name.starts_with(other))
        })
        .map(str::to_owned)
        .collect()
}

/// How long any one `wsl.exe` run may take. The first run starts a stopped
/// distro, which takes seconds; one that hangs (a service wedged, a profile
/// waiting for input) must not hang Dex with it.
const WAIT: Duration = Duration::from_secs(15);

/// Git gets longer: checking out a large repository across the Windows mount
/// is slow, and one killed half-way leaves a worktree half made.
const GIT_WAIT: Duration = Duration::from_secs(600);

/// Runs `wsl.exe` with `args`, hiding the console it would flash open from a
/// GUI app, and gives up after `WAIT`. `None` if it could not be run, or did
/// not finish. `WSL_UTF8` is left out: it makes `--list` print UTF-8 instead
/// of the UTF-16 `parse_distros` reads.
fn run(args: &[&str]) -> Option<Output> {
    run_within(args, WAIT)
}

/// `run`, with its own limit.
fn run_within(args: &[&str], wait: Duration) -> Option<Output> {
    let mut command = Command::new("wsl.exe");
    command
        .args(args)
        .env_remove("WSL_UTF8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().ok()?;
    // Drained as they fill, or a chatty command would block on a full pipe.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + wait;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                tracing::warn!(?args, "wsl.exe did not answer in time");
                return None;
            }
        }
    };
    Some(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

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
pub fn claude_panes(distro: &str) -> Result<Vec<String>, String> {
    let out = run(&["-d", distro, "--exec", "sh", "-c", CLAUDE_PANES])
        .ok_or_else(|| format!("{distro} did not answer"))?;
    if out.status.success() {
        Ok(parse_panes(&String::from_utf8_lossy(&out.stdout)))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// The installed distros agents can run in; empty without WSL (or off Windows).
/// Blocking.
pub fn distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    run(&["--list", "--quiet"])
        .filter(|out| out.status.success())
        .map(|out| parse_distros(&out.stdout))
        .unwrap_or_default()
}

/// A Windows path as `distro` sees it (`C:/src` is `/mnt/c/src`), asked of
/// the distro itself, which knows where it mounts drives. Blocking.
pub fn linux_path(distro: &str, windows: &str) -> Result<String, String> {
    let out = run(&["-d", distro, "--exec", "wslpath", "-u", windows])
        .ok_or_else(|| "wsl.exe could not be run".to_owned())?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.status.success() && path.starts_with('/') {
        Ok(path)
    } else {
        Err(format!(
            "{distro} could not translate {windows}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Runs `git` inside `distro`, in `dir` (a Linux path). Its stdout, or its
/// stderr for `proc::translate`. Blocking.
pub fn git(distro: &str, dir: &str, args: &[&str]) -> Result<String, String> {
    let mut all = vec!["-d", distro, "--exec", "git", "-C", dir];
    all.extend_from_slice(args);
    let out = run_within(&all, GIT_WAIT).ok_or_else(|| {
        format!(
            "git in {distro} did not finish within {} minutes",
            GIT_WAIT.as_secs() / 60
        )
    })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
