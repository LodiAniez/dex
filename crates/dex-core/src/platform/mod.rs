//! Infrastructure shared by every feature. Knows nothing about workspaces,
//! agents, or context — if every feature were deleted, this code would still
//! make sense. Must never import `crate::features` (checked by tests/structure.rs).

pub mod auth;
pub mod bus;
pub mod clock;
pub mod db;
pub mod ids;
pub mod job;
pub mod paths;
pub mod pipe;
pub mod proc;
pub mod pty;
