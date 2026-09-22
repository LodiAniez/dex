//! `AppState`: the platform handles every command handler receives.

use std::path::{Path, PathBuf};

use crate::platform::bus::Bus;
use crate::platform::config::ConfigHandle;
use crate::platform::db::{Db, DbError};
use crate::platform::pty::PtySupervisor;

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
    /// The owner's settings, as they stand right now. Held as a handle rather
    /// than a value because the file is hot-reloaded: take one snapshot per
    /// operation with `config.get()`, never two.
    pub config: ConfigHandle,
}

impl AppState {
    /// Opens (creating and migrating if needed) the database at `db_path`, with
    /// settings already loaded. Anything wrong with the config file is returned
    /// for the caller to show; none of it stops Dex starting.
    pub fn open(db_path: &Path, config_path: PathBuf) -> Result<(Self, Vec<String>), DbError> {
        let (config, problems) = ConfigHandle::load(config_path);
        Ok((Self::with_db(Db::open(db_path)?, config), problems))
    }

    fn with_db(db: Db, config: ConfigHandle) -> Self {
        Self {
            pty: PtySupervisor::new(config.get().flow_limits()),
            db,
            bus: Bus::new(),
            config,
        }
    }
}

#[cfg(test)]
impl AppState {
    /// A state over a fresh temp database at `<dir>/dex.db`. Keep the
    /// returned directory alive for the whole test.
    pub fn for_tests() -> (tempfile::TempDir, Self) {
        Self::for_tests_with(|_| {})
    }

    /// `for_tests`, with the settings changed first.
    pub fn for_tests_with(
        change: impl FnOnce(&mut crate::platform::config::Config),
    ) -> (tempfile::TempDir, Self) {
        use crate::platform::config::Config;

        let (dir, conn) = crate::platform::db::test_db();
        // Worktrees land beside the test database, never in the real home: a
        // bare workspace's root *is* the home directory. A test that unsets
        // this must give its workspace a root of its own.
        let mut config = Config {
            worktree_base: Some(dir.path().join("worktrees")),
            ..Config::default()
        };
        change(&mut config);
        (
            dir,
            Self::with_db(Db::from_connection(conn), ConfigHandle::fixed(config)),
        )
    }
}
