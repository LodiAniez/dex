//! Wire types shared by the Dex app, the `dex` CLI, and the MCP server.
//!
//! Serde types only: no I/O, no logic. Every line on the pipe is one of these,
//! serialized as a single line of JSON (docs/prd.md §6.2).
#![forbid(unsafe_code)]

pub mod agent;
pub mod error;
pub mod handshake;
pub mod pane;
pub mod request;
pub mod response;
pub mod workspace;

pub use error::{ErrorBody, ErrorCode};
pub use handshake::{AuthMessage, Hello, HelloMessage};
pub use request::Request;
pub use response::Response;

/// Protocol version. Client and server must match exactly; `dex doctor` reports a mismatch.
pub const PROTOCOL_VERSION: &str = env!("CARGO_PKG_VERSION");
