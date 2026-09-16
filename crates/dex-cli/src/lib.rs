//! The pipe client, shared by the two thin clients of the running app: the
//! `dex` binary in this crate, and `dex-mcp`.
//!
//! It lives here rather than in its own crate because the repository layout
//! (docs/prd.md §4) fixes the crate list, and because the alternative — a third
//! copy of the HMAC handshake, after `dex_core::platform::auth` and this one —
//! is worse than an odd-looking dependency edge. `dex-mcp` depends on this
//! crate for `client` alone.
#![forbid(unsafe_code)]

pub mod client;
