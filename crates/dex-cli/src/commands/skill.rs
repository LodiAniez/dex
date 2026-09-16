//! `dex skill install | uninstall | status`: the `dex-agentic` skill, which
//! teaches an agent when spawning helps and when it does not (docs/prd.md
//! §12.1), copied into the owner's Claude Code skills folder.
//!
//! The skill ships beside `dex.exe` (`skills\dex-agentic\SKILL.md`, put there
//! by the installer) and is copied, not linked, so an upgrade of Dex changes
//! it only when the owner runs `install` again — `status` says when that is
//! due. Uninstall removes exactly one file and then the folder only if that
//! left it empty; nothing here is recursive.

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::{fs, io};

use clap::Subcommand;
use dex_protocol::{ErrorBody, ErrorCode};
use serde_json::json;

use crate::output::{self, Format};

/// The skill's folder name, under `skills` on both sides.
const NAME: &str = "dex-agentic";
/// The one file a skill is.
const FILE: &str = "SKILL.md";

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Copy (or refresh) the Dex skill into your Claude Code skills.
    Install,
    /// Remove the Dex skill from your Claude Code skills.
    Uninstall,
    /// Show whether it is installed and whether it matches this build.
    Status,
}

/// How the installed copy compares with the one this build ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not installed.
    Missing,
    /// Installed, but not what this build ships.
    Outdated,
    /// Installed and identical to this build's.
    Current,
}

pub fn run(command: SkillCommand, format: Format) -> Result<(), ErrorBody> {
    let source = source()?;
    let target = target_dir()?;
    match command {
        SkillCommand::Install => {
            install(&source, &target)?;
            if format.json {
                output::json(&json!({ "installed": target.join(FILE), "from": source }));
            } else {
                println!(
                    "Installed the {NAME} skill at {}",
                    target.join(FILE).display()
                );
            }
        }
        SkillCommand::Uninstall => {
            let removed = uninstall(&target)?;
            if format.json {
                output::json(&json!({ "removed": removed, "path": target.join(FILE) }));
            } else if removed {
                println!("Removed the {NAME} skill from {}", target.display());
            } else {
                println!("The {NAME} skill was not installed.");
            }
        }
        SkillCommand::Status => {
            let state = state(&source, &target)?;
            if format.json {
                output::json(&json!({
                    "state": format!("{state:?}").to_lowercase(),
                    "path": target.join(FILE),
                    "from": source,
                }));
            } else {
                println!("{}", describe(state, &target));
            }
        }
    }
    Ok(())
}

/// For `dex doctor`: passes only when the installed skill is this build's.
pub fn doctor_check() -> (bool, String) {
    match (source(), target_dir()) {
        (Ok(source), Ok(target)) => match state(&source, &target) {
            Ok(State::Current) => (true, "installed and current".into()),
            Ok(State::Outdated) => (
                false,
                "installed but older than this build (run `dex skill install`)".into(),
            ),
            Ok(State::Missing) => (false, "not installed (run `dex skill install`)".into()),
            Err(err) => (false, err.message),
        },
        (Err(err), _) | (_, Err(err)) => (false, err.message),
    }
}

fn describe(state: State, target: &Path) -> String {
    match state {
        State::Current => format!("Installed at {} and current.", target.join(FILE).display()),
        State::Outdated => format!(
            "Installed at {} but older than this build. Run `dex skill install` to refresh it.",
            target.join(FILE).display()
        ),
        State::Missing => "Not installed. Run `dex skill install` to add it.".into(),
    }
}

/// Where this build's copy of the skill is. Beside `dex.exe` when installed;
/// in a build tree, three levels up from `target/<profile>/dex.exe`.
fn source() -> Result<PathBuf, ErrorBody> {
    let exe = std::env::current_exe()
        .map_err(|err| internal(format!("cannot find this dex.exe: {err}")))?;
    let dir = exe.parent().unwrap_or(Path::new("."));
    source_candidates(dir)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| ErrorBody {
            code: ErrorCode::Internal,
            message: format!(
                "this build has no skills\\{NAME}\\{FILE} beside {}",
                exe.display()
            ),
            repair: "Reinstall Dex; the installer puts the skill beside dex.exe.".into(),
        })
}

/// The places the shipped skill may be, relative to the directory `dex.exe` is in.
pub(crate) fn source_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let tail: PathBuf = ["skills", NAME, FILE].iter().collect();
    vec![
        exe_dir.join(&tail),
        exe_dir.join("..").join("..").join(&tail),
    ]
}

/// `%USERPROFILE%\.claude\skills\dex-agentic`.
fn target_dir() -> Result<PathBuf, ErrorBody> {
    let home = std::env::var_os("USERPROFILE").ok_or_else(|| ErrorBody {
        code: ErrorCode::Internal,
        message: "USERPROFILE is not set".into(),
        repair: "Run this from an ordinary user session.".into(),
    })?;
    Ok(PathBuf::from(home)
        .join(".claude")
        .join("skills")
        .join(NAME))
}

/// Compares the installed file with the shipped one, byte for byte.
pub(crate) fn state(source: &Path, target_dir: &Path) -> Result<State, ErrorBody> {
    let installed = match fs::read(target_dir.join(FILE)) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(State::Missing),
        Err(err) => return Err(unreadable(&target_dir.join(FILE), err)),
    };
    let shipped = fs::read(source).map_err(|err| unreadable(source, err))?;
    Ok(if installed == shipped {
        State::Current
    } else {
        State::Outdated
    })
}

/// Copies the shipped skill over the installed one, creating the folder.
pub(crate) fn install(source: &Path, target_dir: &Path) -> Result<(), ErrorBody> {
    fs::create_dir_all(target_dir)
        .map_err(|err| internal(format!("cannot create {}: {err}", target_dir.display())))?;
    let target = target_dir.join(FILE);
    fs::copy(source, &target)
        .map_err(|err| internal(format!("cannot write {}: {err}", target.display())))?;
    Ok(())
}

/// Removes the skill file, then its folder if that emptied it. Returns whether
/// there was anything to remove.
///
/// Deliberately narrow: one named file, one non-recursive `remove_dir` that
/// fails (harmlessly) if the owner has put anything else in the folder. The
/// folder's name is checked first, so a wrong `target_dir` removes nothing.
pub(crate) fn uninstall(target_dir: &Path) -> Result<bool, ErrorBody> {
    if target_dir.file_name().and_then(|n| n.to_str()) != Some(NAME) {
        return Err(internal(format!(
            "refusing to touch {}: not the {NAME} folder",
            target_dir.display()
        )));
    }
    let file = target_dir.join(FILE);
    match fs::remove_file(&file) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(internal(format!("cannot remove {}: {err}", file.display()))),
    }
    // Only an empty folder goes; anything the owner added stays, and so does the folder.
    let _ = fs::remove_dir(target_dir);
    Ok(true)
}

fn internal(message: String) -> ErrorBody {
    ErrorBody {
        code: ErrorCode::Internal,
        message,
        repair: "Check the path and its permissions, then run this again.".into(),
    }
}

fn unreadable(path: &Path, err: io::Error) -> ErrorBody {
    internal(format!("cannot read {}: {err}", path.display()))
}
