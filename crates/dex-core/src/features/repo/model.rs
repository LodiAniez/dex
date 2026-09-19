//! Domain types owned by the repo slice. Wire types live in dex-protocol.

use thiserror::Error;

use super::logic::BadBranch;
use crate::features::workspace::WorkspaceError;
use crate::platform::db::DbError;
use crate::platform::proc::GitError;

/// A row of the `repo` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Repo id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Absolute path, forward slashes.
    pub path: String,
}

/// Everything that can go wrong in the repo slice. Each variant gets its
/// protocol code and repair string in `router.rs`.
#[derive(Debug, Error)]
pub enum RepoError {
    /// No registered repo matches the target.
    #[error("no repository matches {0:?}")]
    NoSuchRepo(String),
    /// More than one does.
    #[error("{target:?} matches more than one repository")]
    AmbiguousRepo {
        /// What the caller asked for.
        target: String,
        /// The repos it could mean.
        candidates: Vec<String>,
    },
    /// The path is not an existing directory.
    #[error("{0:?} is not an existing directory")]
    NoSuchDir(String),
    /// The branch name breaks git's rules or Windows'.
    #[error("{branch:?} is not a usable branch name")]
    InvalidBranch {
        /// What the caller asked for.
        branch: String,
        /// Which rule it broke.
        reason: BadBranch,
    },
    /// A git too old for a worktree an agent in WSL can use.
    #[error(
        "git in {place} is {version}; a worktree for an agent in WSL needs git 2.48 or later on both sides"
    )]
    GitTooOld {
        /// `Windows`, or the distro's name.
        place: String,
        /// What `git --version` said.
        version: String,
    },
    /// git said no. Translated by `platform::proc`, never raw stderr.
    #[error(transparent)]
    Git(#[from] GitError),
    /// A workspace argument did not resolve.
    #[error(transparent)]
    Target(#[from] WorkspaceError),
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
}
