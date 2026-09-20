//! The single SQLite connection: opening, pragmas, and migrations.
//!
//! Migrations are global, numbered SQL files applied in order and tracked in
//! `schema_version` (docs/conventions.md §1.3). `Db` is the async handle every
//! slice uses; it keeps blocking SQLite calls off the async worker threads.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use thiserror::Error;

/// Every migration, in order. Position + 1 is its schema version.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/001_init.sql"),
    include_str!("../../migrations/002_wake_mark.sql"),
];

/// Failures opening or migrating the database.
#[derive(Debug, Error)]
pub enum DbError {
    /// Any SQLite failure.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The blocking task running the query panicked or was cancelled.
    #[error("database task failed: {0}")]
    Task(String),
}

/// Shared handle to the one SQLite connection. Cheap to clone.
///
/// SQLite calls block, so every call runs on tokio's blocking pool rather than
/// on an async worker thread (docs/conventions.md §4.4). One connection is
/// right for this load; do not build a pool.
#[derive(Debug, Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens (creating if needed) and migrates the database at `path`.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        Ok(Self::from_connection(open(path)?))
    }

    /// Wraps an already-open, already-migrated connection.
    pub fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Runs `work` against the connection on the blocking pool:
    /// `db.call(|conn| store::list_panes(conn, &id)).await`.
    pub async fn call<T, F>(&self, work: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        let joined = tokio::task::spawn_blocking(move || {
            // Locked inside the blocking task, so the guard never lives across an
            // `.await`. A poisoned lock means an earlier query panicked; the
            // connection itself is still usable.
            let mut guard = conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            work(&mut guard)
        })
        .await;
        match joined {
            Ok(result) => Ok(result?),
            Err(err) => Err(DbError::Task(err.to_string())),
        }
    }
}

/// Opens (creating if needed) the database at `path` and brings its schema up to date.
pub fn open(path: &Path) -> Result<Connection, DbError> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<(), DbError> {
    // WAL lets readers proceed while the daemon writes. `journal_mode` returns the
    // resulting mode as a row, so it needs the `_and_check` variant.
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
    // SQLite ships with foreign keys off; every cascade in the schema depends on this.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// Applies every migration newer than the recorded schema version.
fn migrate(conn: &mut Connection) -> Result<(), DbError> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = index as i64 + 1;
        if version <= current {
            continue;
        }
        // The migration and its version bump commit together, so a crash
        // part-way leaves the database at the previous version, not in between.
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [version],
        )?;
        tx.commit()?;
    }
    Ok(())
}

/// Opens a fresh, fully migrated database in a temp directory. The directory
/// is deleted when the returned guard drops, so keep it alive for the test.
#[cfg(test)]
pub fn test_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("temp dir is creatable in tests");
    let conn = open(&dir.path().join("dex.db")).expect("migrations apply to an empty database");
    (dir, conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_version(conn: &Connection) -> i64 {
        conn.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn migrations_bring_an_empty_database_to_the_latest_version() {
        let (_dir, conn) = test_db();
        assert_eq!(schema_version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn reopening_a_migrated_database_is_a_no_op() {
        let (dir, conn) = test_db();
        drop(conn);
        let conn = open(&dir.path().join("dex.db")).unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, MIGRATIONS.len() as i64);
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let (_dir, conn) = test_db();
        let result = conn.execute(
            "INSERT INTO pane (id, workspace_id, cwd, kind, created_at) VALUES ('p', 'missing', 'C:/', 'terminal', 0)",
            [],
        );
        assert!(result.is_err());
    }

    #[test]
    fn fts_index_follows_context_entry_writes() {
        let (_dir, conn) = test_db();
        conn.execute_batch(
            "INSERT INTO workspace (id, name, root_path, sort_index, layout_json, created_at, updated_at)
               VALUES ('w', 'w', 'C:/w', 0, '{}', 0, 0);
             INSERT INTO context_entry (workspace_id, key, value, updated_at)
               VALUES ('w', 'auth/token', 'JWT with fifteen minute expiry', 0);
             UPDATE context_entry SET value = 'opaque session cookies' WHERE key = 'auth/token';",
        )
        .unwrap();
        let hits = |term: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM context_entry_fts WHERE context_entry_fts MATCH ?1",
                [term],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(hits("cookies"), 1);
        assert_eq!(hits("JWT"), 0);
    }
}
