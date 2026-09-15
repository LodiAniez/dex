//! Vertical slices, one per domain. Each owns its types, its SQL, its command
//! handlers, and its tests. Slices call each other only through what a slice's
//! `mod.rs` re-exports (docs/conventions.md §1.2).

mod agent;
mod context;
mod diagnostics;
mod repo;
pub(crate) mod workspace;
