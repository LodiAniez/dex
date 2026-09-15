//! The three-line handshake that opens every connection (docs/prd.md §6.2).
//!
//! Client sends `hello` with a nonce; server answers `hello` with its own nonce
//! and an HMAC proving it knows the token; client finishes with `auth`, proving
//! the same. The HMAC itself lives in `dex-core`'s `platform::auth`.

use serde::{Deserialize, Serialize};

/// Wrapper so the line reads `{"hello":{...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloMessage {
    /// The greeting.
    pub hello: Hello,
}

/// A greeting from either side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Sender's protocol version.
    pub version: String,
    /// Fresh random nonce, hex-encoded, for the other side to sign.
    pub nonce: String,
    /// Server only: HMAC-SHA256(token, client nonce), hex-encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<String>,
}

/// The client's final line: `{"auth":"<hmac of the server nonce>"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMessage {
    /// HMAC-SHA256(token, server nonce), hex-encoded.
    pub auth: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_hello_omits_proof() {
        let msg = HelloMessage {
            hello: Hello {
                version: "1.1.0".into(),
                nonce: "ab".into(),
                proof: None,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"hello":{"version":"1.1.0","nonce":"ab"}}"#);
    }
}
