//! The closed set of error codes, and the error object every failed response carries.

use serde::{Deserialize, Serialize};

/// Every error the daemon can return. Closed on purpose: adding a code is a
/// protocol change, and agents branch on these values (docs/prd.md §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotRunning,
    Unauthorized,
    NotInPane,
    NoSuchWorkspace,
    NoSuchPane,
    NoSuchAgent,
    AmbiguousTarget,
    VersionConflict,
    DepthLimit,
    ConcurrencyLimit,
    GitFailed,
    InvalidArgs,
    Internal,
}

/// The `error` object of a failed response.
///
/// `repair` is a concrete next action in plain language and is never empty:
/// agents read it and retry, so an empty repair turns a recoverable failure
/// into a confused agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ErrorBody {
    /// Machine-readable code.
    pub code: ErrorCode,
    /// What went wrong, for humans and logs.
    pub message: String,
    /// What to do next.
    pub repair: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_serialize_as_snake_case() {
        let json = serde_json::to_string(&ErrorCode::NoSuchWorkspace).unwrap();
        assert_eq!(json, "\"no_such_workspace\"");
    }

    #[test]
    fn unknown_error_code_is_rejected() {
        let parsed: Result<ErrorCode, _> = serde_json::from_str("\"made_up\"");
        assert!(parsed.is_err());
    }
}
