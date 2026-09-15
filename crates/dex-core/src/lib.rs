//! The Dex daemon, embedded in the desktop app.
//!
//! `platform` is infrastructure that knows nothing about features; `features`
//! holds one vertical slice per domain; `router` is the only module that knows
//! every slice exists. See docs/conventions.md before changing structure.
#![forbid(unsafe_code)]

pub mod app;
mod features;
pub mod platform;
pub mod router;
