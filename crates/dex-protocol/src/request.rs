//! The request envelope. Per-command argument structs are added here as commands land.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One request line: `{"id":"req-1","cmd":"pane.create","args":{...}}`.
///
/// `args` stays untyped at the envelope level; each handler deserializes it
/// into its own args struct, so an unknown command never fails to parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Client-chosen id, echoed in the response.
    pub id: String,
    /// Command name, `<slice>.<verb>`.
    pub cmd: String,
    /// Command arguments; `{}` when omitted.
    #[serde(default = "empty_args")]
    pub args: Value,
}

fn empty_args() -> Value {
    Value::Object(serde_json::Map::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_request_parses() {
        let line =
            r#"{"id":"req-1","cmd":"pane.create","args":{"workspace":"w","cwd":"C:/src/api"}}"#;
        let req: Request = serde_json::from_str(line).unwrap();
        assert_eq!(req.cmd, "pane.create");
        assert_eq!(req.args["cwd"], "C:/src/api");
    }

    #[test]
    fn missing_args_default_to_empty_object() {
        let req: Request = serde_json::from_str(r#"{"id":"1","cmd":"pane.id"}"#).unwrap();
        assert!(req.args.as_object().is_some_and(|m| m.is_empty()));
    }
}
