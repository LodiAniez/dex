//! `%APPDATA%\Dex\config.toml`: the settings the owner may change, and the
//! watcher that picks up edits without a restart (docs/prd.md §13).
//!
//! **A bad config never stops Dex starting.** Every field has a default, every
//! section is optional, and a value that makes no sense is replaced by its
//! default and reported rather than refused. On reload, a file that will not
//! parse leaves the running settings exactly as they were: an editor that saves
//! halfway through a keystroke must not reconfigure the app.
//!
//! Keybindings are the exception to "defaults live here": the `[keys]` table
//! holds only overrides, because the defaults are the frontend's and duplicating
//! them in Rust would give the same table two owners.

mod repair;
#[cfg(test)]
mod tests;
mod watch;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::paths;
use super::pty::FlowLimits;

/// Edits that land within this of each other count as one change. Editors write
/// a config file in several operations; reloading on each would reload garbage.
const SETTLE: Duration = Duration::from_millis(250);

/// Everything the owner may configure.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Shell for new panes; the `pwsh`/`powershell`/`cmd` search when unset.
    pub shell: Option<String>,
    /// Where a worktree goes when there is no workspace to go inside
    /// (PRD §8): `dex worktree add` without `--workspace`, a root Windows git
    /// could not work in, or one whose repository will not be told to ignore
    /// it. A workspace's worktrees live in its own root, under
    /// `.dex/worktrees`; a workspace made without a root is rooted at the home
    /// folder, so there that is `~/.dex/worktrees`.
    pub worktree_base: PathBuf,
    /// Loopback port for the WSL TCP fallback (PRD §6.4). Off when unset.
    pub tcp_port: Option<u16>,
    /// Display flow-control watermarks (PRD §7.1).
    pub flow: FlowSettings,
    /// Spawn limits and the mode spawned agents run in (PRD §9.4).
    pub agents: AgentSettings,
    /// Digest budgets (PRD §10.3).
    pub digest: DigestSettings,
    /// The update check.
    pub updates: UpdateSettings,
    /// How the window starts out.
    pub ui: UiSettings,
    /// Keybinding overrides, action to binding. Only what the owner changed.
    pub keys: BTreeMap<String, String>,
}

/// How the window starts out. Dex only reads this: what the owner clicks is
/// remembered by the window, and never written back over their file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    /// How a workspace is shown until the owner chooses: `terminal` (the panes)
    /// or `office` (its agents on a floor plan).
    pub view: String,
}

/// The ways of looking at a workspace.
const VIEWS: [&str; 2] = ["terminal", "office"];
/// A view 0.2.0 had and later versions do not, and what a config asking for it gets.
const REMOVED_VIEW: (&str, &str) = ("cards", "office");

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            view: "terminal".into(),
        }
    }
}

/// Whether the app may ask GitHub for the latest release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateSettings {
    /// One GET to GitHub's releases API at startup and every few hours; a
    /// newer release shows as a pill in the title bar. Nothing is downloaded.
    pub check: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { check: true }
    }
}

/// Watermarks, in bytes of output the display has not acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FlowSettings {
    /// Stop reading the PTY above this.
    pub high_bytes: usize,
    /// Resume below this.
    pub low_bytes: usize,
}

/// Spawn guardrails.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentSettings {
    /// How deep spawning may go.
    pub max_depth: i64,
    /// How many agents may be alive in one workspace.
    pub max_concurrent: i64,
    /// The permission mode spawned agents run in.
    pub spawn_permission_mode: String,
    /// Whether spawned agents may connect to Claude in Chrome. Off: each one
    /// that does brings claude.ai up in the owner's browser.
    pub spawn_chrome: bool,
    /// How long a working agent may go without a hook before Dex says so
    /// (issue #67), in seconds. Hooks fire at every tool batch, so a gap this
    /// long means one tool call has been running all that time. Said, never
    /// acted on, so it is a matter of taste: lower it to hear about a stuck
    /// agent sooner, raise it if long builds make it noise. Between a minute
    /// (below that, ordinary tool calls are called quiet) and a day.
    pub quiet_after_seconds: u64,
}

/// Digest budgets, in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DigestSettings {
    /// Cap on a full digest.
    pub full_chars: usize,
    /// Cap on a delta.
    pub delta_chars: usize,
}

/// Permission modes Claude Code accepts. `bypassPermissions` is allowed here
/// but only honoured for an agent in its own worktree (PRD §9.4).
const PERMISSION_MODES: [&str; 4] = ["default", "acceptEdits", "auto", "bypassPermissions"];

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: None,
            worktree_base: default_worktree_base(),
            tcp_port: None,
            flow: FlowSettings::default(),
            agents: AgentSettings::default(),
            digest: DigestSettings::default(),
            updates: UpdateSettings::default(),
            ui: UiSettings::default(),
            keys: BTreeMap::new(),
        }
    }
}

impl Default for FlowSettings {
    fn default() -> Self {
        let limits = FlowLimits::default();
        Self {
            high_bytes: limits.high,
            low_bytes: limits.low,
        }
    }
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_concurrent: 10,
            spawn_permission_mode: "auto".into(),
            spawn_chrome: false,
            // Long enough that an ordinary build or a CI wait passes without
            // a word, short enough to catch a stuck agent while it matters.
            quiet_after_seconds: 300,
        }
    }
}

impl Default for DigestSettings {
    fn default() -> Self {
        Self {
            full_chars: 2000,
            delta_chars: 800,
        }
    }
}

impl Config {
    /// Parses `text`, replacing values that make no sense with their defaults
    /// and saying so. Only a syntax error fails.
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), toml::de::Error> {
        let mut config: Self = toml::from_str(text)?;
        let problems = config.repair();
        Ok((config, problems))
    }

    /// The watermarks, which panes read when they start.
    pub fn flow_limits(&self) -> FlowLimits {
        FlowLimits {
            high: self.flow.high_bytes,
            low: self.flow.low_bytes,
            ..FlowLimits::default()
        }
    }

    /// Replaces values that make no sense with their defaults, and says which
    /// (`repair.rs`, where the bounds live).
    fn repair(&mut self) -> Vec<String> {
        repair::everything_unusable(self)
    }
}

/// `%USERPROFILE%\dex\worktrees`, or a relative fallback if there is no home.
fn default_worktree_base() -> PathBuf {
    paths::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dex")
        .join("worktrees")
}

/// The live configuration. Cheap to clone; clones share one setting.
#[derive(Clone)]
pub struct ConfigHandle {
    current: Arc<Mutex<Arc<Config>>>,
    /// What was wrong with the file the last time it was read. Kept rather than
    /// only returned, because the app is a GUI: a problem found at startup is
    /// logged where nobody will look, and `dex config show` is where the owner
    /// will actually ask what Dex made of their file.
    problems: Arc<Mutex<Vec<String>>>,
    path: Arc<PathBuf>,
}

impl ConfigHandle {
    /// Reads `path` now. A missing file is not a problem — it means defaults.
    pub fn load(path: PathBuf) -> (Self, Vec<String>) {
        let handle = Self {
            current: Arc::new(Mutex::new(Arc::new(Config::default()))),
            problems: Arc::new(Mutex::new(Vec::new())),
            path: Arc::new(path),
        };
        let problems = handle.reload();
        (handle, problems)
    }

    /// A handle over settings that are never read from disk.
    pub fn fixed(config: Config) -> Self {
        Self {
            current: Arc::new(Mutex::new(Arc::new(config))),
            problems: Arc::new(Mutex::new(Vec::new())),
            path: Arc::new(PathBuf::new()),
        }
    }

    /// What was wrong with the file the last time it was read.
    pub fn problems(&self) -> Vec<String> {
        self.problems
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// The settings as they stand. Take one snapshot per operation rather than
    /// reading twice — a reload between two reads would mix old and new.
    pub fn get(&self) -> Arc<Config> {
        self.lock().clone()
    }

    /// Where the file is, whether or not it exists.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Re-reads the file. Returns what was wrong with it; on a syntax error the
    /// settings in force are left alone.
    pub fn reload(&self) -> Vec<String> {
        let problems = self.read();
        *self
            .problems
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = problems.clone();
        problems
    }

    fn read(&self) -> Vec<String> {
        match std::fs::read_to_string(self.path.as_path()) {
            Ok(text) => match Config::parse(&text) {
                Ok((config, problems)) => {
                    *self.lock() = Arc::new(config);
                    problems
                }
                Err(err) => vec![format!(
                    "{} could not be read, so the settings in force are unchanged: {err}",
                    self.path.display()
                )],
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                *self.lock() = Arc::new(Config::default());
                Vec::new()
            }
            Err(err) => vec![format!(
                "{} could not be opened: {err}",
                self.path.display()
            )],
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Arc<Config>> {
        // A poisoned lock means a thread panicked mid-swap; the value is still
        // a valid `Config`, and refusing to read settings would stop everything.
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// `%APPDATA%\Dex\config.toml`.
pub fn default_path() -> std::io::Result<PathBuf> {
    Ok(paths::app_data_dir()?.join("config.toml"))
}
