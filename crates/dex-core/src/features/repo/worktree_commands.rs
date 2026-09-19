//! `worktree.*`: creating and removing git worktrees (docs/prd.md §8).
//!
//! A worktree is how two agents work on one repository without fighting over
//! the same files. Creation either finishes or fails: a spawn that silently
//! fell back to the main checkout would put two agents in one working tree,
//! which is the exact failure worktrees exist to prevent (§9.4).

use std::path::{Path, PathBuf};

use dex_protocol::repo::{
    AddWorktreeArgs, RemoveWorktreeArgs, RepoArgs, WorktreeList, WorktreeView,
};

use super::commands::{current_branch, resolve};
use super::logic;
use super::model::RepoError;
use super::store;
use crate::app::AppState;
use crate::features::workspace;
use crate::platform::proc::{self, GitError};
use crate::platform::wsl::{self, Runtime};
use crate::platform::{paths, proc::Output};

/// `worktree.add`: a fresh worktree on a new branch, optionally attached to a
/// workspace. Returns every checkout the repo now has.
pub async fn add(state: &AppState, args: AddWorktreeArgs) -> Result<WorktreeList, RepoError> {
    if let Err(reason) = logic::check_branch(&args.branch) {
        return Err(RepoError::InvalidBranch {
            branch: args.branch,
            reason,
        });
    }
    let repo = resolve(state, &args.repo).await?;
    let path = logic::worktree_path(&state.worktree_base(), &repo.name, &args.branch);
    let repo_path = repo.path.clone();
    let branch = args.branch.clone();
    let created: PathBuf = tokio::task::spawn_blocking(move || {
        create(Path::new(&repo_path), &path, &branch)?;
        Ok::<PathBuf, RepoError>(path)
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))??;

    if let Some(target) = args.workspace {
        let normalized = paths::normalize(&created);
        let repo_id = repo.id.clone();
        let branch = args.branch.clone();
        state
            .db
            .call(move |conn| -> rusqlite::Result<Result<(), RepoError>> {
                let workspace_id = match workspace::workspace_id(conn, &target)? {
                    Ok(id) => id,
                    Err(err) => return Ok(Err(err.into())),
                };
                store::attach(
                    conn,
                    &workspace_id,
                    &repo_id,
                    Some(&normalized),
                    Some(&branch),
                )?;
                Ok(Ok(()))
            })
            .await??;
    }
    list(state, RepoArgs { repo: repo.path }).await
}

/// `worktree.remove`: take a worktree away and forget it.
pub async fn remove(state: &AppState, args: RemoveWorktreeArgs) -> Result<WorktreeList, RepoError> {
    let repo = resolve(state, &args.repo).await?;
    let path = logic::worktree_path(&state.worktree_base(), &repo.name, &args.branch);
    let repo_path = repo.path.clone();
    let force = args.force;
    tokio::task::spawn_blocking(move || {
        let mut command = vec!["worktree", "remove"];
        if force {
            command.push("--force");
        }
        let path = path.to_string_lossy().into_owned();
        command.push(&path);
        proc::git(Path::new(&repo_path), &command)
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))??;

    let repo_id = repo.id.clone();
    let branch = args.branch.clone();
    state
        .db
        .call(move |conn| store::detach(conn, &repo_id, &branch))
        .await?;
    list(state, RepoArgs { repo: repo.path }).await
}

/// `worktree.list`: every checkout git knows for a repo, main first.
pub async fn list(state: &AppState, args: RepoArgs) -> Result<WorktreeList, RepoError> {
    let repo = resolve(state, &args.repo).await?;
    tokio::task::spawn_blocking(move || {
        let dir = Path::new(&repo.path);
        let out = proc::git(dir, &["worktree", "list", "--porcelain"])?;
        let worktrees = logic::parse_worktrees(&out.stdout)
            .into_iter()
            .enumerate()
            .map(|(index, (path, branch))| WorktreeView {
                // git lists the main checkout first.
                main: index == 0,
                path: paths::normalize(Path::new(&path)),
                branch,
            })
            .collect();
        Ok(WorktreeList { worktrees })
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))?
}

/// Creates the worktree, reusing the branch when it already exists.
///
/// `-b` fails on a branch that is already there, which would stop an agent
/// resuming work it started earlier; falling back to a plain checkout of that
/// branch is what the caller meant either way.
fn create(repo: &Path, path: &Path, branch: &str) -> Result<Output, RepoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))?;
    }
    let path = path.to_string_lossy().into_owned();
    match proc::git(repo, &["worktree", "add", &path, "-b", branch]) {
        Err(GitError::Exists(_)) => proc::git(repo, &["worktree", "add", &path, branch]),
        other => other,
    }
    .map_err(RepoError::Git)
}

/// `create`, by the distro's own git, for an agent that runs in WSL. Windows
/// git would record the worktree's links as `C:/...` paths, which Linux git
/// cannot follow, and check files out with Windows line endings, which it sees
/// as every line changed. Linux git with `--relative-paths` avoids both, and
/// Windows git reads the result as well (both need git 2.48 or later).
fn create_in_wsl(distro: &str, repo: &Path, path: &Path, branch: &str) -> Result<(), RepoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))?;
    }
    let linux = |windows: &Path| {
        wsl::linux_path(distro, &paths::normalize(windows))
            .map_err(|err| RepoError::Git(GitError::Other(err)))
    };
    let (repo, path) = (linux(repo)?, linux(path)?);
    let add = |new_branch: bool| {
        let mut args = vec!["worktree", "add", "--relative-paths", &path];
        args.extend(if new_branch {
            vec!["-b", branch]
        } else {
            vec![branch]
        });
        wsl::git(distro, &repo, &args).map_err(|stderr| proc::translate(&stderr))
    };
    match add(true) {
        Err(GitError::Exists(_)) => add(false),
        other => other,
    }
    .map(|_| ())
    .map_err(RepoError::Git)
}

/// For the agent slice: creating a worktree as part of a spawn.
///
/// Spawning cannot go through `worktree.add`: it needs the path back, and a
/// spawn that fell back to the main checkout on failure would put two agents
/// in one working tree (PRD §9.4). This fails outright instead.
pub async fn create_for_spawn(
    state: &AppState,
    repo_target: &str,
    branch: &str,
    runtime: &str,
) -> Result<(String, PathBuf), RepoError> {
    if let Err(reason) = logic::check_branch(branch) {
        return Err(RepoError::InvalidBranch {
            branch: branch.to_owned(),
            reason,
        });
    }
    let repo = resolve(state, repo_target).await?;
    let path = logic::worktree_path(&state.worktree_base(), &repo.name, branch);
    let repo_path = repo.path.clone();
    let branch = branch.to_owned();
    let made = path.clone();
    let runtime = Runtime::parse(runtime).unwrap_or(Runtime::Windows);
    tokio::task::spawn_blocking(move || match runtime {
        Runtime::Wsl(distro) => create_in_wsl(&distro, Path::new(&repo_path), &made, &branch),
        Runtime::Windows => create(Path::new(&repo_path), &made, &branch).map(|_| ()),
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))??;
    Ok((repo.id, path))
}

/// For other slices: the branch checked out in a directory.
pub fn branch_at(dir: &Path) -> Option<String> {
    current_branch(dir)
}

/// For the agent slice: records which worktree a workspace now uses, inside the
/// spawn's own transaction.
pub fn attach_worktree(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    repo_id: &str,
    worktree_path: &str,
    branch: Option<&str>,
) -> rusqlite::Result<()> {
    store::attach(conn, workspace_id, repo_id, Some(worktree_path), branch)
}
