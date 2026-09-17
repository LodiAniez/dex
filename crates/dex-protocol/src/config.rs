//! Wire types for `config.get` and `config.reload` (docs/prd.md §13).
//!
//! The settings themselves are the daemon's — their shape belongs in
//! `dex_core::platform::config`, next to the defaults and the validation. What
//! crosses the pipe is what a client actually does something with: where the
//! file is, what is wrong with it, the keybinding overrides the UI merges onto
//! its own defaults, the few values the UI draws from, and the effective settings as text for a human to read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The settings in force.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ConfigView {
    /// Where the file is, whether or not it exists.
    pub path: String,
    /// Whether it exists. A missing file means defaults, which is not an error.
    pub present: bool,
    /// Keybinding overrides, action to binding. Only what the owner changed:
    /// the defaults are the frontend's, so that table has one owner.
    pub keys: BTreeMap<String, String>,
    /// How many agents may be alive in one workspace: the office's seats.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub max_concurrent: i64,
    /// How a workspace is shown until the owner chooses: `terminal` or `office`.
    pub view: String,
    /// What Dex could not use, and what it used instead. Empty when the file is
    /// good, or absent.
    pub problems: Vec<String>,
    /// The effective settings as TOML, for a human to read. Generated from the
    /// values in force, so it shows the defaults filled in.
    pub effective: String,
}
