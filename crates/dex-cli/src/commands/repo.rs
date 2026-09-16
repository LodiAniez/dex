//! `dex repo …` and `dex worktree …`: the repository registry and the git
//! worktrees agents work in (docs/prd.md §11).

use std::path::PathBuf;

use clap::Subcommand;
use dex_protocol::ErrorBody;
use dex_protocol::repo::{RepoList, RepoStatus, WorktreeList};
use serde_json::json;

use crate::commands::workspace::absolute;
use crate::output::{self, Format};
use dex_cli::client;

#[derive(Debug, Subcommand)]
pub enum RepoCommand {
    /// Register an existing git repository.
    Add {
        /// Its path; the current directory by default.
        path: Option<PathBuf>,
        /// Display name; the directory name by default.
        #[arg(long)]
        name: Option<String>,
    },
    /// List the registered repositories and their branches.
    List,
    /// Find git repositories under a directory and register them.
    Scan {
        /// Where to look.
        path: PathBuf,
        /// How deep; 3 by default.
        #[arg(long)]
        depth: Option<u32>,
    },
    /// Show a repository's branch and working-tree counts.
    Status {
        /// Repo name, id, or path.
        repo: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum WorktreeCommand {
    /// Create a worktree on a new branch.
    Add {
        /// Repo name, id, or path.
        #[arg(long)]
        repo: String,
        /// Branch to create and check out.
        #[arg(long)]
        branch: String,
        /// Attach it to this workspace.
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Remove a worktree.
    Remove {
        /// Repo name, id, or path.
        #[arg(long)]
        repo: String,
        /// Branch whose worktree goes.
        #[arg(long)]
        branch: String,
        /// Remove it even when it holds uncommitted work.
        #[arg(long)]
        force: bool,
    },
    /// List a repository's checkouts.
    List {
        /// Repo name, id, or path.
        repo: String,
    },
}

pub fn run(command: RepoCommand, format: Format) -> Result<(), ErrorBody> {
    let mut client = client::connect()?;
    match command {
        RepoCommand::Add { path, name } => {
            let path = absolute(path.unwrap_or_else(|| PathBuf::from(".")));
            let list: RepoList = client.call("repo.add", json!({ "path": path, "name": name }))?;
            print_repos(&list, format);
        }
        RepoCommand::List => {
            let list: RepoList = client.call("repo.list", json!({}))?;
            print_repos(&list, format);
        }
        RepoCommand::Scan { path, depth } => {
            let list: RepoList = client.call(
                "repo.scan",
                json!({ "path": absolute(path), "depth": depth }),
            )?;
            print_repos(&list, format);
        }
        RepoCommand::Status { repo } => {
            let status: RepoStatus = client.call("repo.status", json!({ "repo": repo }))?;
            if format.json {
                output::json(&status);
            } else {
                println!(
                    "{}  +{} ~{} -{}",
                    status.branch.as_deref().unwrap_or("(detached)"),
                    status.added,
                    status.modified,
                    status.deleted
                );
            }
        }
    }
    Ok(())
}

pub fn run_worktree(command: WorktreeCommand, format: Format) -> Result<(), ErrorBody> {
    let mut client = client::connect()?;
    let list: WorktreeList = match command {
        WorktreeCommand::Add {
            repo,
            branch,
            workspace,
        } => client.call(
            "worktree.add",
            json!({ "repo": repo, "branch": branch, "workspace": workspace }),
        )?,
        WorktreeCommand::Remove {
            repo,
            branch,
            force,
        } => client.call(
            "worktree.remove",
            json!({ "repo": repo, "branch": branch, "force": force }),
        )?,
        WorktreeCommand::List { repo } => client.call("worktree.list", json!({ "repo": repo }))?,
    };
    if format.json {
        output::json(&list);
        return Ok(());
    }
    let rows: Vec<Vec<String>> = list
        .worktrees
        .iter()
        .map(|wt| {
            vec![
                if wt.main {
                    "main".into()
                } else {
                    String::new()
                },
                wt.branch.clone().unwrap_or_else(|| "(detached)".into()),
                wt.path.clone(),
            ]
        })
        .collect();
    output::table(format, &["", "BRANCH", "PATH"], &rows);
    Ok(())
}

fn print_repos(list: &RepoList, format: Format) {
    if format.json {
        return output::json(list);
    }
    let rows: Vec<Vec<String>> = list
        .repos
        .iter()
        .map(|repo| {
            vec![
                repo.name.clone(),
                repo.branch.clone().unwrap_or_else(|| "(detached)".into()),
                repo.path.clone(),
            ]
        })
        .collect();
    output::table(format, &["NAME", "BRANCH", "PATH"], &rows);
}
