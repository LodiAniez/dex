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

use std::fmt;

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
/// `cwd` (a Windows path, which `wsl.exe` translates).
pub fn pane_args(distro: &str, cwd: &str) -> Vec<String> {
    ["-d", distro, "--cd", cwd].map(str::to_owned).to_vec()
}

/// The distros `wsl.exe -l -q` printed, less Docker Desktop's own.
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
        .filter(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
        .map(str::to_owned)
        .collect()
}

/// Runs `wsl.exe` with `args`, hiding the console it would flash open from a
/// GUI app. `None` if it could not be run.
fn run(args: &[&str]) -> Option<std::process::Output> {
    let mut command = std::process::Command::new("wsl.exe");
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.output().ok()
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
    let out = run(&all).ok_or_else(|| "wsl.exe could not be run".to_owned())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
