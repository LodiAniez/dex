//! One handler per `repo.*` command: the registry, discovery, and status
//! (docs/prd.md §8). Worktrees live in `worktree_commands.rs`.
//!
//! Every git call goes through `platform::proc`, on the blocking pool: git is a
//! subprocess and can take as long as the repository is large.

use std::path::{Path, PathBuf};

use dex_protocol::repo::{
    AddRepoArgs, DiffArgs, RepoArgs, RepoDiff, RepoList, RepoStatus, RepoView, ScanArgs,
};
use rusqlite::Connection;

use super::logic;
use super::model::{Repo, RepoError};
use super::store;
use crate::app::AppState;
use crate::platform::proc::{self, GitError};
use crate::platform::{clock, ids, paths};

/// How deep `repo.scan` looks when the caller does not say.
const DEFAULT_SCAN_DEPTH: u32 = 3;
/// Directories never worth descending into while scanning.
const SKIP: [&str; 7] = [
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    ".venv",
    // Dex's own: a workspace's agent worktrees live in `.dex/worktrees`, and
    // scanning the workspace must not register each of them as a repository.
    ".dex",
];

/// `repo.add`: register an existing git repository.
pub async fn add(state: &AppState, args: AddRepoArgs) -> Result<RepoList, RepoError> {
    let path = existing_dir(&args.path)?;
    // Reject a non-repository here rather than at the first worktree, when the
    // caller has already built a workspace around it.
    let root = tokio::task::spawn_blocking(move || repo_root(&path))
        .await
        .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))??;
    let name = args
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| logic::repo_name(Path::new(&root)));
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| {
            store::insert_repo(
                conn,
                &Repo {
                    id: ids::new_id(),
                    name,
                    path: root,
                },
                now,
            )
        })
        .await?;
    list(state).await
}

/// `repo.list`: every registered repo, with the branch each has checked out.
pub async fn list(state: &AppState) -> Result<RepoList, RepoError> {
    let repos = state.db.call(|conn| store::list_repos(conn)).await?;
    let repos = tokio::task::spawn_blocking(move || {
        repos
            .into_iter()
            .map(|repo| {
                let branch = current_branch(Path::new(&repo.path));
                RepoView {
                    id: repo.id,
                    name: repo.name,
                    path: repo.path,
                    branch,
                }
            })
            .collect()
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))?;
    Ok(RepoList { repos })
}

/// `repo.scan`: find git repositories under a directory and register them.
pub async fn scan(state: &AppState, args: ScanArgs) -> Result<RepoList, RepoError> {
    let root = existing_dir(&args.path)?;
    let depth = args.depth.unwrap_or(DEFAULT_SCAN_DEPTH);
    let found = tokio::task::spawn_blocking(move || discover(Path::new(&root), depth))
        .await
        .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))?;
    let now = clock::now_millis();
    state
        .db
        .call(move |conn| {
            for path in found {
                let name = logic::repo_name(Path::new(&path));
                store::insert_repo(
                    conn,
                    &Repo {
                        id: ids::new_id(),
                        name,
                        path,
                    },
                    now,
                )?;
            }
            Ok(())
        })
        .await?;
    list(state).await
}

/// `repo.status`: branch and working-tree counts, for the status bar.
pub async fn status(state: &AppState, args: RepoArgs) -> Result<RepoStatus, RepoError> {
    let repo = resolve(state, &args.repo).await?;
    tokio::task::spawn_blocking(move || {
        let dir = Path::new(&repo.path);
        let counts = proc::git(dir, &["status", "--porcelain=v1"])
            .map(|out| logic::parse_status(&out.stdout))
            .unwrap_or_default();
        Ok(RepoStatus {
            added: counts.added,
            modified: counts.modified,
            deleted: counts.deleted,
            branch: current_branch(dir),
        })
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))?
}

/// The most of a diff the pane will show. Past this it is a review nobody will
/// read in a pane; the reader is told it was cut and can run git themselves.
const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;

/// `repo.diff`: the working-tree (or staged) diff of the repository holding
/// `path`, for the diff pane. Any directory in the repository will do — git
/// finds the root — so a pane's cwd is enough, registered or not.
pub async fn diff(state: &AppState, args: DiffArgs) -> Result<RepoDiff, RepoError> {
    let _ = state;
    let dir = existing_dir(&args.path)?;
    tokio::task::spawn_blocking(move || {
        let dir = Path::new(&dir);
        let mut git_args = vec!["diff", "--no-color", "--no-ext-diff"];
        if args.staged {
            git_args.push("--cached");
        }
        let out = proc::git(dir, &git_args)?;
        let truncated = out.stdout.len() > MAX_DIFF_BYTES;
        let text = if truncated {
            // Cut on a line, so the last thing shown is a whole line.
            let cut = out.stdout[..MAX_DIFF_BYTES]
                .rfind('\n')
                .unwrap_or(MAX_DIFF_BYTES);
            out.stdout[..cut].to_owned()
        } else {
            out.stdout
        };
        Ok(RepoDiff {
            text,
            truncated,
            branch: current_branch(dir),
        })
    })
    .await
    .map_err(|err| RepoError::Git(GitError::Other(err.to_string())))?
}

/// The repo a target names: its id, its exact name, else a unique prefix.
pub(super) async fn resolve(state: &AppState, target: &str) -> Result<Repo, RepoError> {
    let wanted = target.to_owned();
    state
        .db
        .call(move |conn| Ok(pick(&store::list_repos(conn)?, &wanted)))
        .await?
}

fn pick(repos: &[Repo], target: &str) -> Result<Repo, RepoError> {
    // Path as well as id and name: commands that already hold a repo pass its
    // path around, and a human naturally reaches for one too.
    if let Some(exact) = repos
        .iter()
        .find(|repo| repo.id == target || repo.name == target || repo.path == target)
    {
        return Ok(exact.clone());
    }
    let matches: Vec<&Repo> = repos
        .iter()
        .filter(|repo| repo.name.eq_ignore_ascii_case(target))
        .collect();
    match matches.as_slice() {
        [only] => Ok((*only).clone()),
        [] => Err(RepoError::NoSuchRepo(target.to_owned())),
        many => Err(RepoError::AmbiguousRepo {
            target: target.to_owned(),
            candidates: many
                .iter()
                .map(|repo| format!("{} ({})", repo.name, repo.path))
                .collect(),
        }),
    }
}

/// The repository root containing `dir`, as git reports it.
fn repo_root(dir: &Path) -> Result<String, RepoError> {
    let out = proc::git(dir, &["rev-parse", "--show-toplevel"])?;
    Ok(paths::normalize(Path::new(out.stdout.trim())))
}

/// The branch checked out in `dir`, or `None` when detached or not a repo.
pub(super) fn current_branch(dir: &Path) -> Option<String> {
    let out = proc::git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let branch = out.stdout.trim();
    (!branch.is_empty() && branch != "HEAD").then(|| branch.to_owned())
}

/// Git repositories under `root`, down to `depth`, not descending into one.
fn discover(root: &Path, depth: u32) -> Vec<String> {
    let mut found = Vec::new();
    walk(root, depth, &mut found);
    found
}

fn walk(dir: &Path, depth: u32, found: &mut Vec<String>) {
    if dir.join(".git").exists() {
        // A repository's own subdirectories are its business, not ours.
        found.push(paths::normalize(dir));
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let skip = path
            .file_name()
            .map(|name| SKIP.contains(&name.to_string_lossy().as_ref()))
            .unwrap_or(true);
        if !skip && path.is_dir() {
            walk(&path, depth - 1, found);
        }
    }
}

/// An existing directory, normalized, or an error naming what was asked for.
pub(super) fn existing_dir(path: &str) -> Result<PathBuf, RepoError> {
    let candidate = PathBuf::from(path);
    if !candidate.is_dir() {
        return Err(RepoError::NoSuchDir(path.to_owned()));
    }
    Ok(candidate)
}

/// One of a workspace's repos: its name, where it is checked out (its
/// worktree, else the repo itself), and the branch recorded when it was linked.
pub type WorkspaceRepo = (String, Option<String>, Option<String>);

/// For other slices: the repos a workspace uses and where each is checked out.
pub fn workspace_repos(
    conn: &Connection,
    workspace_id: &str,
) -> rusqlite::Result<Vec<WorkspaceRepo>> {
    Ok(store::repos_of_workspace(conn, workspace_id)?
        .into_iter()
        .map(|(repo, worktree, branch)| (repo.name, worktree.or(Some(repo.path)), branch))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str, path: &str) -> Repo {
        Repo {
            id: format!("id-{name}"),
            name: name.into(),
            path: path.into(),
        }
    }

    #[test]
    fn a_repo_resolves_by_id_name_path_then_case_insensitively() {
        let repos = vec![repo("api", "C:/src/api"), repo("web", "C:/src/web")];
        assert_eq!(pick(&repos, "id-api").unwrap().name, "api");
        assert_eq!(pick(&repos, "web").unwrap().name, "web");
        assert_eq!(pick(&repos, "C:/src/api").unwrap().name, "api");
        assert_eq!(pick(&repos, "API").unwrap().name, "api");
    }

    #[test]
    fn an_unknown_repo_is_an_error_naming_what_was_asked_for() {
        let repos = vec![repo("api", "C:/src/api")];
        assert!(matches!(
            pick(&repos, "nope"),
            Err(RepoError::NoSuchRepo(ref target)) if target == "nope"
        ));
    }

    #[test]
    fn two_repos_with_the_same_name_list_both_paths() {
        // Two checkouts of the same project, registered from different places.
        let repos = vec![repo("api", "C:/work/api"), repo("API", "D:/old/api")];
        match pick(&repos, "api") {
            Ok(found) => assert_eq!(found.path, "C:/work/api", "an exact name still wins"),
            other => panic!("expected the exact match, got {other:?}"),
        }
        match pick(&repos, "aPi") {
            Err(RepoError::AmbiguousRepo { candidates, .. }) => {
                assert_eq!(candidates.len(), 2);
                assert!(candidates.iter().any(|c| c.contains("C:/work/api")));
                assert!(candidates.iter().any(|c| c.contains("D:/old/api")));
            }
            other => panic!("expected an ambiguity, got {other:?}"),
        }
    }
}
