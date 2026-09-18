//! `dex hooks install | uninstall | status`: Dex's Claude Code hooks in a
//! Claude Code settings file (docs/prd.md §9.3).
//!
//! Merging rule: a hook entry is Dex's if it runs `dex`/`dex.exe` with `event`
//! as its first argument. Install removes Dex's entries and adds the current
//! set; everything else in the file — other hooks, other settings, key order —
//! is left exactly as found. So it is safe to run again, after an upgrade or
//! when dex.exe moves.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use dex_protocol::{ErrorBody, ErrorCode};
use serde_json::{Map, Value, json};

use crate::output::{self, Format};

/// One hook Dex installs: the Claude Code event (and matcher), the `dex event`
/// kind it runs, and whether Claude Code waits for it. Only hooks whose output
/// must reach the model before it continues (digests, from M6) wait; status
/// updates run in the background (PRD §9.3).
struct Hook {
    event: &'static str,
    matcher: Option<&'static str>,
    kind: &'static str,
    wait: bool,
}

const HOOKS: [Hook; 10] = [
    Hook {
        event: "SessionStart",
        matcher: None,
        kind: "session-start",
        wait: true,
    },
    Hook {
        event: "UserPromptSubmit",
        matcher: None,
        kind: "prompt",
        wait: true,
    },
    Hook {
        event: "PostToolBatch",
        matcher: None,
        kind: "batch",
        wait: true,
    },
    Hook {
        event: "PermissionRequest",
        matcher: None,
        kind: "permission",
        wait: false,
    },
    Hook {
        event: "Notification",
        matcher: Some("permission_prompt"),
        kind: "waiting",
        wait: false,
    },
    Hook {
        event: "Notification",
        matcher: Some("agent_needs_input"),
        kind: "waiting",
        wait: false,
    },
    Hook {
        event: "Notification",
        matcher: Some("idle_prompt"),
        kind: "idle",
        wait: false,
    },
    Hook {
        event: "Stop",
        matcher: None,
        kind: "stop",
        wait: false,
    },
    Hook {
        event: "StopFailure",
        matcher: None,
        kind: "stop-failure",
        wait: false,
    },
    Hook {
        event: "SessionEnd",
        matcher: None,
        kind: "session-end",
        wait: false,
    },
];

#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    /// Add (or refresh) Dex's hooks. Your other hooks are kept.
    Install {
        /// Settings file (default: %USERPROFILE%\.claude\settings.json).
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    /// Remove Dex's hooks, leaving every other hook in place.
    Uninstall {
        /// Settings file (default: %USERPROFILE%\.claude\settings.json).
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    /// Show whether Dex's hooks are installed and point at this dex.exe.
    Status {
        /// Settings file (default: %USERPROFILE%\.claude\settings.json).
        #[arg(long)]
        settings: Option<PathBuf>,
    },
}

pub fn run(command: HooksCommand, format: Format) -> Result<(), ErrorBody> {
    let exe = current_exe()?;
    match command {
        HooksCommand::Install { settings } => {
            let path = settings_path(settings)?;
            let mut doc = read(&path)?;
            let backup = backup(&path)?;
            install(&mut doc, &exe).map_err(|shape| malformed(&path, shape))?;
            write(&path, &doc)?;
            if format.json {
                output::json(
                    &json!({ "installed": HOOKS.len(), "settings": path, "backup": backup }),
                );
            } else {
                println!(
                    "Installed {} Dex hooks into {}",
                    HOOKS.len(),
                    path.display()
                );
                if let Some(backup) = backup {
                    println!(
                        "The file as it was before Dex is saved at {}",
                        backup.display()
                    );
                }
            }
        }
        HooksCommand::Uninstall { settings } => {
            let path = settings_path(settings)?;
            let mut doc = read(&path)?;
            let removed = remove(&mut doc).map_err(|shape| malformed(&path, shape))?;
            write(&path, &doc)?;
            if format.json {
                output::json(&json!({ "removed": removed, "settings": path }));
            } else {
                println!("Removed {removed} Dex hooks from {}", path.display());
            }
        }
        HooksCommand::Status { settings } => {
            let path = settings_path(settings)?;
            let found = status(&read(&path)?, &exe);
            if format.json {
                output::json(
                    &json!({ "expected": HOOKS.len(), "installed": found.dex, "current": found.current, "settings": path }),
                );
            } else {
                println!("{}", describe(&found));
            }
        }
    }
    Ok(())
}

/// For `dex doctor`: are all of Dex's hooks installed, pointing at this dex.exe?
pub fn doctor_check() -> (bool, String) {
    let found = match (current_exe(), settings_path(None)) {
        (Ok(exe), Ok(path)) => match read(&path) {
            Ok(doc) => status(&doc, &exe),
            Err(err) => return (false, format!("{} {}", err.message, err.repair)),
        },
        (Err(err), _) | (_, Err(err)) => return (false, err.message),
    };
    let ok = found.current == HOOKS.len();
    let detail = if ok {
        describe(&found)
    } else {
        format!("{} (run `dex hooks install`)", describe(&found))
    };
    (ok, detail)
}

/// What a settings file holds of Dex's hooks.
struct Found {
    /// Entries that are Dex's.
    dex: usize,
    /// Of those, entries that run this dex.exe.
    current: usize,
}

fn describe(found: &Found) -> String {
    if found.current == HOOKS.len() {
        format!("all {} Dex hooks installed", HOOKS.len())
    } else if found.dex > found.current {
        format!(
            "{} Dex hooks point at another dex.exe (moved or old install)",
            found.dex - found.current
        )
    } else {
        format!("{} of {} Dex hooks installed", found.current, HOOKS.len())
    }
}

/// What part of a settings file had an unexpected shape.
#[derive(Debug, PartialEq, Eq)]
struct Malformed(&'static str);

/// Whether a hook entry is Dex's: `dex`/`dex.exe` run with `event` first.
fn is_dex_hook(hook: &Value) -> bool {
    let command = hook.get("command").and_then(Value::as_str).unwrap_or("");
    // Split on both separators: a Windows path is written with `\`, which a
    // Unix `Path` does not treat as one.
    let program = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let first_arg = hook
        .get("args")
        .and_then(Value::as_array)
        .and_then(|args| args.first())
        .and_then(Value::as_str);
    (program == "dex.exe" || program == "dex") && first_arg == Some("event")
}

/// Removes Dex's entries, and any group or event list left empty by that.
/// Returns how many entries were removed.
fn remove(doc: &mut Value) -> Result<usize, Malformed> {
    let root = doc
        .as_object_mut()
        .ok_or(Malformed("the file is not a JSON object"))?;
    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(0);
    };
    let events = hooks
        .as_object_mut()
        .ok_or(Malformed("\"hooks\" is not an object"))?;
    let mut removed = 0;
    for groups in events.values_mut() {
        // Shapes we do not understand are left alone.
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        for group in groups.iter_mut() {
            if let Some(entries) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = entries.len();
                entries.retain(|entry| !is_dex_hook(entry));
                removed += before - entries.len();
            }
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|e| !e.is_empty())
        });
    }
    events.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    let now_empty = events.is_empty();
    if now_empty {
        root.remove("hooks");
    }
    Ok(removed)
}

/// Replaces Dex's entries with the current set, pointing at `exe`.
fn install(doc: &mut Value, exe: &str) -> Result<(), Malformed> {
    remove(doc)?;
    let root = doc
        .as_object_mut()
        .ok_or(Malformed("the file is not a JSON object"))?;
    let events = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or(Malformed("\"hooks\" is not an object"))?;
    for hook in &HOOKS {
        let groups = events
            .entry(hook.event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or(Malformed("an event's hook list is not an array"))?;
        // Exec form (`args`): no shell, so no quoting problems with spaces in paths.
        let mut entry = json!({ "type": "command", "command": exe, "args": ["event", hook.kind] });
        if !hook.wait {
            entry["async"] = Value::Bool(true);
        }
        let mut group = Map::new();
        if let Some(matcher) = hook.matcher {
            group.insert("matcher".into(), matcher.into());
        }
        group.insert("hooks".into(), Value::Array(vec![entry]));
        groups.push(Value::Object(group));
    }
    Ok(())
}

fn status(doc: &Value, exe: &str) -> Found {
    let mut found = Found { dex: 0, current: 0 };
    let entries = doc
        .get("hooks")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|events| events.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten();
    for entry in entries.filter(|entry| is_dex_hook(entry)) {
        found.dex += 1;
        if entry.get("command").and_then(Value::as_str) == Some(exe) {
            found.current += 1;
        }
    }
    found
}

fn current_exe() -> Result<String, ErrorBody> {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|err| file_error(format!("cannot find this dex.exe: {err}")))
}

fn settings_path(requested: Option<PathBuf>) -> Result<PathBuf, ErrorBody> {
    if let Some(path) = requested {
        return Ok(path);
    }
    dex_cli::paths::home_dir()
        .map(|home| home.join(".claude").join("settings.json"))
        .ok_or_else(|| {
            file_error("no home folder: USERPROFILE (Windows) or HOME is not set".into())
        })
}

/// The settings document; a missing file is an empty one.
fn read(path: &Path) -> Result<Value, ErrorBody> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Value::Object(Map::new())),
        Ok(text) => serde_json::from_str(&text).map_err(|err| ErrorBody {
            code: ErrorCode::InvalidArgs,
            message: format!("{} is not valid JSON: {err}", path.display()),
            repair: "Fix the file (Claude Code cannot read it either), then run this again.".into(),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(err) => Err(file_error(format!("cannot read {}: {err}", path.display()))),
    }
}

/// Saves the file as it was before Dex first touched it, once. Later installs
/// keep that original.
fn backup(path: &Path) -> Result<Option<PathBuf>, ErrorBody> {
    let backup = PathBuf::from(format!("{}.dex-backup", path.display()));
    if !path.exists() {
        return Ok(None);
    }
    if !backup.exists() {
        fs::copy(path, &backup)
            .map_err(|err| file_error(format!("cannot back up {}: {err}", path.display())))?;
    }
    Ok(Some(backup))
}

/// Writes through a temporary file and a rename, so a crash never leaves a
/// half-written settings file behind.
fn write(path: &Path, doc: &Value) -> Result<(), ErrorBody> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|err| file_error(format!("cannot create {}: {err}", dir.display())))?;
    }
    let text = serde_json::to_string_pretty(doc).map_err(|err| file_error(err.to_string()))? + "\n";
    let temp = PathBuf::from(format!("{}.dex-tmp", path.display()));
    fs::write(&temp, text)
        .map_err(|err| file_error(format!("cannot write {}: {err}", temp.display())))?;
    fs::rename(&temp, path)
        .map_err(|err| file_error(format!("cannot replace {}: {err}", path.display())))
}

fn malformed(path: &Path, shape: Malformed) -> ErrorBody {
    ErrorBody {
        code: ErrorCode::InvalidArgs,
        message: format!("{} has an unexpected shape: {}", path.display(), shape.0),
        repair: "Fix that part of the file by hand; Dex only edits hook lists it understands."
            .into(),
    }
}

fn file_error(message: String) -> ErrorBody {
    ErrorBody {
        code: ErrorCode::Internal,
        message,
        repair: "Check the path and its permissions, then run this again.".into(),
    }
}

#[cfg(test)]
mod tests;
