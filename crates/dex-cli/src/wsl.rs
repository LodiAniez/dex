//! WSL, as the CLI needs it: which distros there are, where things are inside
//! one, and running a program there. The daemon has its own copy of the list
//! (`dex_core::platform::wsl`); the CLI does not depend on the daemon.
//!
//! Files inside a distro are read and written from Windows through
//! `\\wsl.localhost\<distro>` (`\\wsl$\<distro>` on Windows 10 before 21H2),
//! so the owner's Claude Code settings in Linux are edited by the same code,
//! with the same backups, as on Windows.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// Distros that are some other program's machinery, not a place to work.
const NOT_FOR_AGENTS: [&str; 3] = ["docker-desktop", "rancher-desktop", "podman-machine"];

/// The distros `wsl.exe -l -q` printed (UTF-16LE), less other programs' own.
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

/// How long any one `wsl.exe` run may take: the first starts a stopped distro,
/// which takes seconds, but a profile waiting for input must not hang
/// `dex doctor`, which the app runs at every start.
const WAIT: Duration = Duration::from_secs(15);

/// `wsl.exe`, without the console it would flash open from the app, given up
/// on after `WAIT`. `WSL_UTF8` is left out: it makes `--list` print UTF-8
/// instead of the UTF-16 `parse_distros` reads.
fn wsl(args: &[&str]) -> io::Result<Output> {
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
    let mut child = command.spawn()?;
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
    let deadline = Instant::now() + WAIT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("wsl.exe did not answer within {} seconds", WAIT.as_secs()),
            ));
        }
        thread::sleep(Duration::from_millis(20));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
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
    let out = out.map_err(|err| format!("{what} failed: {err}"))?;
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

/// `DEX_WSL_HOME`, when set: a Linux folder that stands in for the owner's
/// home in the distro, for an isolated test that must not touch theirs - the
/// files Dex writes, and what the distro's `claude` reads and writes.
fn test_home() -> Option<String> {
    std::env::var("DEX_WSL_HOME")
        .ok()
        .filter(|home| home.starts_with('/'))
}

/// The owner's home folder in the distro.
pub fn home(distro: &str) -> Result<String, String> {
    if let Some(home) = test_home() {
        return Ok(home);
    }
    printed(
        wsl(&["-d", distro, "--exec", "printenv", "HOME"]),
        "finding the home folder",
    )
}

/// Where a folder really is, following symbolic links (`realpath -m`).
pub fn real_path(distro: &str, linux: &str) -> Result<String, String> {
    printed(
        wsl(&["-d", distro, "--exec", "realpath", "-m", linux]),
        "resolving a path",
    )
}

/// A Linux path under `root` (`\\wsl.localhost\<distro>`), as Windows reaches it.
pub fn under(root: &Path, linux: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    path.extend(linux.split('/').filter(|part| !part.is_empty()));
    path
}

/// Where Windows reaches the distro's files: `\\wsl.localhost\<distro>`, or
/// `\\wsl$\<distro>`, the older name, where Windows has only that. Asked once
/// per distro that is being set up, not per file.
pub fn root(distro: &str) -> PathBuf {
    let current = PathBuf::from(format!(r"\\wsl.localhost\{distro}"));
    if current.is_dir() {
        current
    } else {
        PathBuf::from(format!(r"\\wsl$\{distro}"))
    }
}

/// Printed by the owner's shell before its `PATH`, so a greeting from their
/// profile (Ubuntu's once-a-day welcome, say) is not mistaken for part of it.
const MARKER: &str = "__DEX_PATH__";

/// What follows the marker on its line, if anything does.
pub fn read_marked(printed: &str) -> Option<String> {
    let after = printed.split(MARKER).nth(1)?;
    let line = after.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// The owner's login shell in the distro, from the user database. Shells whose
/// `$PATH` is not a colon-separated string (fish, nu) are asked through bash.
fn login_shell(distro: &str) -> String {
    let shell = printed(
        wsl(&[
            "-d",
            distro,
            "--exec",
            "sh",
            "-c",
            r#"getent passwd "$(id -un)" | cut -d: -f7"#,
        ]),
        "finding the login shell",
    )
    .unwrap_or_default();
    let name = shell.rsplit('/').next().unwrap_or_default();
    if shell.starts_with('/') && !matches!(name, "fish" | "nu") {
        shell
    } else {
        "/bin/bash".to_owned()
    }
}

/// The `PATH` the owner's shell sets for a terminal in the distro: asked once
/// of an interactive login shell - where nvm, Homebrew and `~/.local/bin` are
/// added - which prints it and nothing else Dex reads. With the owner's real
/// home even in a test: that is where their Claude Code is.
pub fn login_path(distro: &str) -> Option<String> {
    static ASKED: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let asked = ASKED.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(known) = asked.lock().ok().and_then(|map| map.get(distro).cloned()) {
        return known;
    }
    let shell = login_shell(distro);
    let script = format!(r#"printf '{MARKER}%s\n' "$PATH""#);
    let path = wsl(&["-d", distro, "--exec", &shell, "-lic", &script])
        .ok()
        .and_then(|out| read_marked(&String::from_utf8_lossy(&out.stdout)));
    if let Ok(mut map) = asked.lock() {
        map.insert(distro.to_owned(), path.clone());
    }
    path
}

/// The environment a program in the distro runs with: the owner's terminal
/// `PATH`, and in a test the stand-in home, whose `~/.local/bin` comes first.
pub fn login_env(path: Option<&str>, test_home: Option<&str>) -> Vec<String> {
    let path = path.unwrap_or("/usr/local/bin:/usr/bin:/bin");
    match test_home {
        Some(home) => vec![
            format!("PATH={home}/.local/bin:{path}"),
            format!("HOME={home}"),
        ],
        None => vec![format!("PATH={path}")],
    }
}

/// Runs `program` in the distro as a terminal of the owner's would find it -
/// Claude Code installed per user, `dex` in `~/.local/bin` - but without an
/// interactive shell, whose greetings would land in what is read back.
pub fn run_login(distro: &str, program: &str, args: &[&str]) -> io::Result<Output> {
    let env = login_env(login_path(distro).as_deref(), test_home().as_deref());
    let mut all = vec!["-d", distro, "--exec", "env"];
    all.extend(env.iter().map(String::as_str));
    all.push(program);
    all.extend_from_slice(args);
    wsl(&all)
}

/// Makes `path` in the distro runnable.
pub fn make_executable(distro: &str, path: &str) -> Result<(), String> {
    let out = wsl(&["-d", distro, "--exec", "chmod", "755", path])
        .map_err(|err| format!("chmod {path} failed: {err}"))?;
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
