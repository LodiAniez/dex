//! Every SQL statement touching `repo` and `workspace_repo`. No other module
//! queries them (docs/conventions.md §1.3).

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::model::Repo;

const REPO_COLUMNS: &str = "id, name, path";

fn repo_from_row(row: &Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
    })
}

/// Registers a repo. The path is unique, so re-adding one is a no-op.
pub fn insert_repo(conn: &Connection, repo: &Repo, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO repo (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO NOTHING",
        params![repo.id, repo.name, repo.path, now],
    )?;
    Ok(())
}

/// Every repo, by name.
pub fn list_repos(conn: &Connection) -> rusqlite::Result<Vec<Repo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {REPO_COLUMNS} FROM repo ORDER BY name, id"
    ))?;
    let rows = stmt.query_map([], repo_from_row)?;
    rows.collect()
}

/// Records which worktree of a repo a workspace uses.
pub fn attach(
    conn: &Connection,
    workspace_id: &str,
    repo_id: &str,
    worktree_path: Option<&str>,
    branch: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO workspace_repo (workspace_id, repo_id, worktree_path, branch)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(workspace_id, repo_id) DO UPDATE SET
           worktree_path = excluded.worktree_path, branch = excluded.branch",
        params![workspace_id, repo_id, worktree_path, branch],
    )?;
    Ok(())
}

/// Where Dex recorded the worktree it made for `branch` of a repo, if it did.
pub fn worktree_of(
    conn: &Connection,
    repo_id: &str,
    branch: &str,
) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT worktree_path FROM workspace_repo
         WHERE repo_id = ?1 AND branch = ?2 AND worktree_path IS NOT NULL LIMIT 1",
        params![repo_id, branch],
        |row| row.get(0),
    )
    .optional()
}

/// Forgets a workspace's use of a repo.
pub fn detach(conn: &Connection, repo_id: &str, branch: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM workspace_repo WHERE repo_id = ?1 AND branch = ?2",
        params![repo_id, branch],
    )
}

/// A repo as one workspace uses it: the repo, its worktree path, its branch.
pub type RepoUse = (Repo, Option<String>, Option<String>);

/// The repos a workspace uses.
pub fn repos_of_workspace(conn: &Connection, workspace_id: &str) -> rusqlite::Result<Vec<RepoUse>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.name, r.path, wr.worktree_path, wr.branch
         FROM workspace_repo AS wr JOIN repo AS r ON r.id = wr.repo_id
         WHERE wr.workspace_id = ?1 ORDER BY r.name",
    )?;
    let rows = stmt.query_map([workspace_id], |row| {
        Ok((repo_from_row(row)?, row.get(3)?, row.get(4)?))
    })?;
    rows.collect()
}
