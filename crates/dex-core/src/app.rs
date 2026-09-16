//! `AppState`: the platform handles every command handler receives.

use std::path::{Path, PathBuf};

use crate::platform::bus::Bus;
use crate::platform::db::{Db, DbError};
use crate::platform::paths;
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
    /// Where git worktrees are created (PRD §8): `%USERPROFILE%\dex\worktrees`
    /// unless overridden. Held here rather than read from the environment so
    /// tests can point it somewhere harmless, and so M8 can make it
    /// configurable without touching the repo slice.
    pub worktree_base: PathBuf,
}

impl AppState {
    /// Opens (creating and migrating if needed) the database at `db_path`.
    pub fn open(db_path: &Path) -> Result<Self, DbError> {
        Ok(Self::with_db(Db::open(db_path)?, default_worktree_base()))
    }

    fn with_db(db: Db, worktree_base: PathBuf) -> Self {
        Self {
            db,
            pty: PtySupervisor::new(FlowLimits::default()),
            bus: Bus::new(),
            worktree_base,
        }
    }
}

/// `%USERPROFILE%\dex\worktrees`, or a relative fallback if there is no home.
fn default_worktree_base() -> PathBuf {
    paths::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dex")
        .join("worktrees")
}

#[cfg(test)]
impl AppState {
    /// A state over a fresh temp database at `<dir>/dex.db`. Keep the
    /// returned directory alive for the whole test.
    pub fn for_tests() -> (tempfile::TempDir, Self) {
        let (dir, conn) = crate::platform::db::test_db();
        // Worktrees land beside the test database, never in the real home.
        let worktrees = dir.path().join("worktrees");
        (dir, Self::with_db(Db::from_connection(conn), worktrees))
    }
}
