//! Domain types owned by the diagnostics slice. Wire types live in dex-protocol, not here.

use thiserror::Error;

/// Failures from the diagnostics commands.
#[derive(Debug, Error)]
pub enum DiagnosticsError {
    /// The settings in force could not be rendered back to TOML. Only reachable
    /// through a bug: every value came from TOML in the first place.
    #[error("the settings could not be printed: {0}")]
    Unprintable(String),
}
