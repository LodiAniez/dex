//! Identifier generation.

/// A new random identifier (UUID v4) — the id format of every table (docs/prd.md §5).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
