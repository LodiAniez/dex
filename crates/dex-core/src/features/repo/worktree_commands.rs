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
use super::placement::{self, MadeBy};
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
    let workspace = match args.workspace.as_deref() {
        Some(target) => Some(workspace_and_root(state, target).await?),
        None => None,
    };
    let root = workspace.as_ref().and_then(|(_, root)| root.clone());
    let outside = state.worktree_base();
    let (name, repo_path) = (repo.name.clone(), repo.path.clone());
    let branch = args.branch.clone();
    let created: PathBuf = tokio::task::spawn_blocking(move || {
        let path = placement::place(&outside, root.as_deref(), MadeBy::Windows, &name, &branch);
        create(Path::new(&repo_path), &path, &branch)?;
        Ok::<PathBuf, RepoError>(path)
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))??;

    if let Some((workspace_id, _)) = workspace {
        let normalized = paths::normalize(&created);
        let repo_id = repo.id.clone();
        let branch = args.branch.clone();
        state
            .db
            .call(move |conn| {
                store::attach(
                    conn,
                    &workspace_id,
                    &repo_id,
                    Some(&normalized),
                    Some(&branch),
                )
            })
            .await?;
    }
    list(state, RepoArgs { repo: repo.path }).await
}

/// The workspace a target names, and its root.
async fn workspace_and_root(
    state: &AppState,
    target: &str,
) -> Result<(String, Option<String>), RepoError> {
    let target = target.to_owned();
    state
        .db
        .call(
            move |conn| -> rusqlite::Result<Result<(String, Option<String>), RepoError>> {
                let id = match workspace::workspace_id(conn, &target)? {
                    Ok(id) => id,
                    Err(err) => return Ok(Err(err.into())),
                };
                let root = workspace::workspace_root(conn, &id)?;
                Ok(Ok((id, root)))
            },
        )
        .await?
}

/// `worktree.remove`: take a worktree away and forget it.
pub async fn remove(state: &AppState, args: RemoveWorktreeArgs) -> Result<WorktreeList, RepoError> {
    let repo = resolve(state, &args.repo).await?;
    let (repo_id, branch) = (repo.id.clone(), args.branch.clone());
    let recorded = state
        .db
        .call(move |conn| store::worktrees_of(conn, &repo_id, &branch))
        .await?;
    let (repo_path, name) = (repo.path.clone(), repo.name.clone());
    let (branch, force) = (args.branch.clone(), args.force);
    tokio::task::spawn_blocking(move || {
        let dir = Path::new(&repo_path);
        if let Some(path) = checkout_of(dir, &branch, &recorded)? {
            return take_away(dir, &path, force);
        }
        // Git has lost it. A folder still there may hold work git no longer
        // knows of, so it is not Dex's to delete: the owner is told where it
        // is. With the folders gone too - deleted and pruned by hand - all
        // that is left is Dex's record, forgotten below.
        if let Some(left) = recorded.iter().find(|at| Path::new(at).exists()) {
            return Err(RepoError::WorktreeLeftBehind { path: left.clone() });
        }
        if recorded.is_empty() {
            return Err(RepoError::NoSuchWorktree { repo: name, branch });
        }
        Ok(())
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

/// `git worktree remove`, forced if asked.
fn take_away(repo: &Path, path: &str, force: bool) -> Result<(), RepoError> {
    let mut command = vec!["worktree", "remove"];
    if force {
        command.push("--force");
    }
    command.push(path);
    proc::git(repo, &command)
        .map(|_| ())
        .map_err(RepoError::Git)
}

/// Where the worktree for `branch` is, asked of git rather than worked out: it
/// went inside its workspace or to `worktree_base` depending on the
/// workspace's root, and everything made before went to `worktree_base`.
///
/// Where Dex recorded making it comes first, if git still has it there: the
/// agent may have switched branch since, or be mid-rebase with none. Compared
/// as places, not as strings - a root written `c:\code\.` records a path git
/// prints as `C:/code`. The branch after. The main checkout is never one.
fn checkout_of(
    repo: &Path,
    branch: &str,
    recorded: &[String],
) -> Result<Option<String>, RepoError> {
    let listed = proc::git(repo, &["worktree", "list", "--porcelain"])?;
    let worktrees: Vec<(String, Option<String>)> = logic::parse_worktrees(&listed.stdout)
        .into_iter()
        .skip(1)
        .collect();
    let at_recorded = worktrees.iter().find(|(path, _)| {
        recorded
            .iter()
            .any(|at| placement::same_place(Path::new(path), Path::new(at)))
    });
    let on_branch = || {
        worktrees
            .iter()
            .find(|(_, on)| on.as_deref() == Some(branch))
    };
    Ok(at_recorded.or_else(on_branch).map(|(path, _)| path.clone()))
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
        placement::make_dir(parent)?;
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
        placement::make_dir(parent)?;
    }
    let linux = |windows: &Path| {
        wsl::linux_path(distro, &paths::normalize(windows))
            .map_err(|err| RepoError::Git(GitError::Other(err)))
    };
    // Git on both sides must know relative worktrees: adding one marks the
    // repository's format, and an older git then refuses the whole repository.
    let too_old = |place: &str, printed: &str| match logic::git_version(printed) {
        Some(version) if version >= logic::RELATIVE_WORKTREES => Ok(()),
        _ => Err(RepoError::GitTooOld {
            place: place.to_owned(),
            version: printed.trim().trim_start_matches("git version ").to_owned(),
        }),
    };
    let windows = proc::git(repo, &["--version"]).map_err(RepoError::Git)?;
    too_old("Windows", &windows.stdout)?;
    let (repo, path) = (linux(repo)?, linux(path)?);
    match wsl::git_version(distro) {
        Ok(Some(linux_git)) => too_old(distro, &linux_git)?,
        Ok(None) => {
            return Err(RepoError::GitTooOld {
                place: distro.to_owned(),
                version: "not installed".to_owned(),
            });
        }
        Err(err) => return Err(RepoError::Git(GitError::Other(err))),
    }
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

/// For the agent slice: creating a worktree as part of a spawn - inside the
/// workspace it is for when that is a plain folder, else at `worktree_base`.
///
/// Spawning cannot go through `worktree.add`: it needs the path back, and a
/// spawn that fell back to the main checkout on failure would put two agents
/// in one working tree (PRD §9.4). This fails outright instead.
pub async fn create_for_spawn(
    state: &AppState,
    repo_target: &str,
    branch: &str,
    runtime: &str,
    workspace_id: &str,
) -> Result<(String, PathBuf), RepoError> {
    if let Err(reason) = logic::check_branch(branch) {
        return Err(RepoError::InvalidBranch {
            branch: branch.to_owned(),
            reason,
        });
    }
    let repo = resolve(state, repo_target).await?;
    let id = workspace_id.to_owned();
    let root = state
        .db
        .call(move |conn| workspace::workspace_root(conn, &id))
        .await?;
    let outside = state.worktree_base();
    let (name, repo_path) = (repo.name.clone(), repo.path.clone());
    let branch = branch.to_owned();
    let runtime = Runtime::parse(runtime).map_err(|err| {
        RepoError::Target(crate::features::workspace::WorkspaceError::InvalidRuntime(
            err,
        ))
    })?;
    let path = tokio::task::spawn_blocking(move || {
        let made_by = match runtime {
            Runtime::Wsl(_) => MadeBy::Wsl,
            Runtime::Windows => MadeBy::Windows,
        };
        let path = placement::place(&outside, root.as_deref(), made_by, &name, &branch);
        match runtime {
            Runtime::Wsl(distro) => create_in_wsl(&distro, Path::new(&repo_path), &path, &branch)?,
            Runtime::Windows => {
                create(Path::new(&repo_path), &path, &branch)?;
            }
        }
        Ok::<PathBuf, RepoError>(path)
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
