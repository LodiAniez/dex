//! WSL, as the CLI needs it: which distros there are, where things are inside
//! one, and running a program there. The daemon has its own copy of the list
//! (`dex_core::platform::wsl`); the CLI does not depend on the daemon.
//!
//! Files inside a distro are read and written from Windows through
//! `\\wsl.localhost\<distro>`, so the owner's Claude Code settings in Linux are
//! edited by the same code, with the same backups, as on Windows.

#[cfg(test)]
mod tests;

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The distros `wsl.exe -l -q` printed (UTF-16LE), less Docker Desktop's own.
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

/// `wsl.exe`, without the console it would flash open from the app.
fn wsl(args: &[&str]) -> io::Result<Output> {
    let mut command = Command::new("wsl.exe");
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.output()
}

/// The installed distros; empty without WSL, or off Windows.
pub fn distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    wsl(&["--list", "--quiet"])
        .ok()
        .filter(|out| out.status.success())
        .map(|out| parse_distros(&out.stdout))
        .unwrap_or_default()
}

/// What a command printed, trimmed, or why it failed.
fn printed(out: io::Result<Output>, what: &str) -> Result<String, String> {
    let out = out.map_err(|err| format!("could not run wsl.exe: {err}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.status.success() && !text.is_empty() {
        Ok(text)
    } else {
        Err(format!(
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// A Windows path as the distro sees it (`C:\x` is `/mnt/c/x`).
pub fn linux_path(distro: &str, windows: &Path) -> Result<String, String> {
    let windows = windows.to_string_lossy();
    printed(
        wsl(&["-d", distro, "--exec", "wslpath", "-u", &windows]),
        "translating a path",
    )
}

/// The owner's home folder in the distro. `DEX_WSL_HOME` stands in for it,
/// for an isolated test that must not touch the owner's own Linux home.
pub fn home(distro: &str) -> Result<String, String> {
    if let Some(home) = std::env::var("DEX_WSL_HOME")
        .ok()
        .filter(|home| home.starts_with('/'))
    {
        return Ok(home);
    }
    printed(
        wsl(&["-d", distro, "--exec", "printenv", "HOME"]),
        "finding the home folder",
    )
}

/// A Linux path as Windows reaches it: `\\wsl.localhost\<distro>\home\...`.
pub fn unc(distro: &str, linux: &str) -> PathBuf {
    PathBuf::from(format!(
        r"\\wsl.localhost\{distro}{}",
        linux.replace('/', "\\")
    ))
}

/// Runs `program` in the distro with the `PATH` a login shell sets, where a
/// Claude Code installed per user lives (`~/.local/bin`). The arguments go to
/// the program as they are: the shell only runs `exec "$0" "$@"`.
pub fn run_login(distro: &str, program: &str, args: &[&str]) -> io::Result<Output> {
    let mut all = vec![
        "-d",
        distro,
        "--exec",
        "bash",
        "-lc",
        "exec \"$0\" \"$@\"",
        program,
    ];
    all.extend_from_slice(args);
    wsl(&all)
}

/// Makes `path` in the distro runnable.
pub fn make_executable(distro: &str, path: &str) -> Result<(), String> {
    let out = wsl(&["-d", distro, "--exec", "chmod", "755", path])
        .map_err(|err| format!("could not run wsl.exe: {err}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "chmod {path} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The `dex` command inside Linux: runs the Windows `dex.exe` at `target`
/// (a Linux path) with whatever it was given.
pub fn shim(target: &str) -> String {
    let quoted = target.replace('\'', r"'\''");
    format!(
        "#!/bin/sh\n# Written by `dex wsl setup`: Dex's CLI is the Windows one.\nexec '{quoted}' \"$@\"\n"
    )
}
