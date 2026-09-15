//! `AppState`: the platform handles every command handler receives.

use std::path::Path;

use crate::platform::bus::Bus;
use crate::platform::db::{Db, DbError};
use crate::platform::pty::{FlowLimits, PtySupervisor};

/// Handles to the long-lived platform services. Cheap to clone; every clone
/// shares the same services.
#[derive(Clone)]
pub struct AppState {
    /// The database.
    pub db: Db,
    /// Every live pane's process.
    pub pty: PtySupervisor,
    /// Change notifications for the UI.
    pub bus: Bus,
}

impl AppState {
    /// Opens (creating and migrating if needed) the database at `db_path`.
    pub fn open(db_path: &Path) -> Result<Self, DbError> {
        Ok(Self::with_db(Db::open(db_path)?))
    }

    fn with_db(db: Db) -> Self {
        Self {
            db,
            pty: PtySupervisor::new(FlowLimits::default()),
            bus: Bus::new(),
        }
    }
}

#[cfg(test)]
impl AppState {
    /// A state over a fresh temp database at `<dir>/dex.db`. Keep the
    /// returned directory alive for the whole test.
    pub fn for_tests() -> (tempfile::TempDir, Self) {
        let (dir, conn) = crate::platform::db::test_db();
        (dir, Self::with_db(Db::from_connection(conn)))
    }
}
