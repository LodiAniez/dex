//! The response envelope.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ErrorBody;

/// One response line. Exactly one of `data` (when `ok`) or `error` is present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// The request's id.
    pub id: String,
    /// Whether the command succeeded.
    pub ok: bool,
    /// Result payload on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Error on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Response {
    /// A successful response carrying `data`.
    pub fn success(id: impl Into<String>, data: Value) -> Self {
        Self {
            id: id.into(),
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    /// A failed response carrying `error`.
    pub fn failure(id: impl Into<String>, error: ErrorBody) -> Self {
        Self {
            id: id.into(),
            ok: false,
            data: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn failure_matches_the_documented_wire_shape() {
        let resp = Response::failure(
            "req-1",
            ErrorBody {
                code: ErrorCode::NoSuchWorkspace,
                message: "no workspace named x".into(),
                repair: "Run `dex workspace list` to see valid names.".into(),
            },
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(
            json,
            r#"{"id":"req-1","ok":false,"error":{"code":"no_such_workspace","message":"no workspace named x","repair":"Run `dex workspace list` to see valid names."}}"#
        );
    }

    #[test]
    fn success_omits_error() {
        let json =
            serde_json::to_string(&Response::success("1", serde_json::json!({"pane_id":"p"})))
                .unwrap();
        assert_eq!(json, r#"{"id":"1","ok":true,"data":{"pane_id":"p"}}"#);
    }
}
