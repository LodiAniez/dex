//! `AppState`: the platform handles every command handler receives.

use std::path::Path;

use crate::platform::db::{Db, DbError};

/// Handles to the long-lived platform services. Cheap to clone; fields are
/// added as the platform modules that commands need land.
#[derive(Debug, Clone)]
pub struct AppState {
    /// The database.
    pub db: Db,
}

impl AppState {
    /// Opens (creating and migrating if needed) the database at `db_path`.
    pub fn open(db_path: &Path) -> Result<Self, DbError> {
        Ok(Self {
            db: Db::open(db_path)?,
        })
    }
}

#[cfg(test)]
impl AppState {
    /// A state over a fresh temp database at `<dir>/dex.db`. Keep the
    /// returned directory alive for the whole test.
    pub fn for_tests() -> (tempfile::TempDir, Self) {
        let (dir, conn) = crate::platform::db::test_db();
        (
            dir,
            Self {
                db: Db::from_connection(conn),
            },
        )
    }
}
