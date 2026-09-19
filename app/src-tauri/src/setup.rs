//! First-run setup (PRD §14 M8): what is not yet set up, and the buttons that
//! set it up.
//!
//! Nothing here is a second implementation. The checks are `dex doctor --json`
//! and the fixes are `dex hooks install` and `dex mcp install`, run as the
//! human would run them, so the panel can never disagree with the CLI and a
//! fix applied from the panel is exactly the one the docs describe. The
//! steps are a closed list - three, and one per installed WSL distro, named
//! `wsl:<distro>` and checked against `wsl.exe --list`: the UI names one, it
//! never passes arguments.

use std::path::PathBuf;
use std::process::Command;

use dex_core::platform::wsl;
use serde::{Deserialize, Serialize};

/// Hides the console window a console program would otherwise flash open
/// when started from a GUI process (`CREATE_NO_WINDOW`).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `dex doctor --json`, as printed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub ok: bool,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub status: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

/// What running a setup step printed, and whether it worked.
#[derive(Debug, Clone, Serialize)]
pub struct StepOutcome {
    pub ok: bool,
    pub output: String,
}

/// `dex` beside this executable — where the installer puts it (inside the
/// app bundle on macOS) and where a build leaves it — or, failing that,
/// whatever `dex` PATH finds.
fn dex_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|dir| dir.join(format!("dex{}", std::env::consts::EXE_SUFFIX)))
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("dex"))
}

fn dex(args: &[&str]) -> Result<std::process::Output, String> {
    let mut command = Command::new(dex_exe());
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    // On macOS the app has a bare PATH, and `dex` runs `claude` and `git`.
    if let Some(path) = dex_core::platform::login_env::login_path() {
        command.env("PATH", path);
    }
    command
        .output()
        .map_err(|err| format!("could not run dex: {err}"))
}

/// Runs `dex doctor --json` and returns its report.
#[tauri::command]
pub async fn setup_check() -> Result<Report, String> {
    // A subprocess, off the async threads like every other blocking call.
    let output = tauri::async_runtime::spawn_blocking(|| dex(&["doctor", "--json"]))
        .await
        .map_err(|err| err.to_string())??;
    // Doctor exits non-zero when a check fails; its JSON is still the answer.
    serde_json::from_slice(&output.stdout).map_err(|err| {
        format!(
            "dex doctor gave no report: {err}. It printed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })
}

/// Runs one setup step: `hooks`, `mcp`, `skill`, or `wsl:<distro>` for an
/// installed distro. Anything else is refused.
#[tauri::command]
pub async fn setup_run(step: String) -> Result<StepOutcome, String> {
    let output = tauri::async_runtime::spawn_blocking(move || {
        let args: Vec<String> = match step.as_str() {
            "hooks" | "mcp" | "skill" => vec![step.clone(), "install".into()],
            other => match other.strip_prefix("wsl:") {
                // Only a distro `wsl.exe` lists: the name becomes an argument.
                Some(distro) if wsl::distros().iter().any(|d| d == distro) => {
                    vec!["wsl".into(), "setup".into(), distro.to_owned()]
                }
                _ => return Err(format!("{other:?} is not a setup step")),
            },
        };
        dex(&args.iter().map(String::as_str).collect::<Vec<_>>())
    })
    .await
    .map_err(|err| err.to_string())??;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&stderr);
    }
    Ok(StepOutcome {
        ok: output.status.success(),
        output: text,
    })
}
