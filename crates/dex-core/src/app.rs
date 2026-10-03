//! `AppState`: the platform handles every command handler receives.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
    /// When the last occasional pass ran, for `claim_slot`. In memory on
    /// purpose: after a restart the first pass runs once, which costs one
    /// question to each remote and keeps the state out of the database.
    slot_at: Arc<Mutex<i64>>,
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
            slot_at: Arc::new(Mutex::new(0)),
        }
    }

    /// Whether `every` milliseconds have passed since the last claim, claiming
    /// the slot if so: for a pass that must run occasionally while being
    /// driven by a sweep that runs every fifteen seconds.
    ///
    /// The lock is taken and dropped here and never held across an await
    /// (conventions 4.4). A poisoned lock answers no, which skips the pass
    /// rather than panicking inside a sweep.
    pub fn claim_slot(&self, now: i64, every: i64) -> bool {
        let Ok(mut last) = self.slot_at.lock() else {
            return false;
        };
        if now.saturating_sub(*last) < every {
            return false;
        }
        *last = now;
        true
    }

    /// Where git worktrees are created (PRD §8).
    pub fn worktree_base(&self) -> PathBuf {
        self.config.get().worktree_base.clone()
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
        // Worktrees land beside the test database, never in the real home.
        let mut config = Config {
            worktree_base: dir.path().join("worktrees"),
            ..Config::default()
        };
        change(&mut config);
        (
            dir,
            Self::with_db(Db::from_connection(conn), ConfigHandle::fixed(config)),
        )
    }
}
