//! One module per command family, mirroring the daemon's feature slices.

pub mod agent;
pub mod config;
pub mod context;
pub mod doctor;
pub mod event;
pub mod hooks;
pub mod mcp;
pub mod pane;
pub mod repo;
pub mod skill;
pub mod workspace;
pub mod wsl;
