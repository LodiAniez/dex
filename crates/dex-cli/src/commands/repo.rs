//! `dex repo …` and `dex worktree …`: the repository registry and the git
//! worktrees agents work in (docs/prd.md §11).

use std::path::PathBuf;

use clap::Subcommand;
use dex_protocol::ErrorBody;
use dex_protocol::repo::{KeptBecause, Pruned, RepoList, RepoStatus, WorktreeList, WorktreeView};
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
        /// Measure what each one is holding on disk.
        #[arg(long)]
        sizes: bool,
    },
    /// Remove the worktrees nobody is working in any more.
    ///
    /// A worktree goes only when no pane has a shell running in it, its tree
    /// is clean, and its branch has nothing the main checkout's branch has
    /// not. Everything else is left where it is, with the reason why.
    Prune {
        /// Repo name, id, or path.
        repo: String,
        /// Say what would go without taking anything away.
        #[arg(long)]
        dry_run: bool,
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
    if let WorktreeCommand::Prune { repo, dry_run } = command {
        let pruned: Pruned = client.call(
            "worktree.prune",
            json!({ "repo": repo, "dry_run": dry_run }),
        )?;
        print_pruned(&pruned, format);
        return Ok(());
    }
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
        WorktreeCommand::List { repo, sizes } => {
            client.call("worktree.list", json!({ "repo": repo, "sizes": sizes }))?
        }
        WorktreeCommand::Prune { .. } => unreachable!("answered above"),
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
                branch_of(wt),
                held(wt.size_bytes),
                wt.path.clone(),
            ]
        })
        .collect();
    output::table(format, &["", "BRANCH", "SIZE", "PATH"], &rows);
    Ok(())
}

/// What a prune did, or would do: what went, then what stayed and why.
fn print_pruned(pruned: &Pruned, format: Format) {
    if format.json {
        return output::json(pruned);
    }
    println!(
        "{} {} worktree{}, {}{}",
        if pruned.dry_run { "would take" } else { "took" },
        pruned.taken.len(),
        if pruned.taken.len() == 1 { "" } else { "s" },
        held(Some(pruned.freed_bytes)),
        if pruned.dry_run { " (dry run)" } else { "" },
    );
    if !pruned.taken.is_empty() {
        let rows: Vec<Vec<String>> = pruned
            .taken
            .iter()
            .map(|wt| vec![branch_of(wt), held(wt.size_bytes), wt.path.clone()])
            .collect();
        output::table(format, &["BRANCH", "SIZE", "PATH"], &rows);
    }
    if pruned.kept.is_empty() {
        return;
    }
    println!();
    println!("kept {}:", pruned.kept.len());
    let rows: Vec<Vec<String>> = pruned
        .kept
        .iter()
        .map(|kept| {
            vec![
                branch_of(&kept.worktree),
                why(kept.because).to_owned(),
                kept.worktree.path.clone(),
            ]
        })
        .collect();
    output::table(format, &["BRANCH", "WHY", "PATH"], &rows);
}

fn branch_of(worktree: &WorktreeView) -> String {
    worktree
        .branch
        .clone()
        .unwrap_or_else(|| "(detached)".into())
}

/// Why a worktree stayed, in the owner's terms rather than the protocol's.
fn why(because: KeptBecause) -> &'static str {
    match because {
        KeptBecause::InUse => "a shell is running in it",
        KeptBecause::Uncommitted => "uncommitted work",
        KeptBecause::Unmerged => "commits the main branch has not",
        KeptBecause::Detached => "no branch checked out",
        KeptBecause::Unknown => "git could not compare its branch",
        KeptBecause::Refused => "git would not remove it",
    }
}

/// What a worktree is holding, to one decimal place and never as bytes: this
/// is read to decide what to delete, and `1.2 GB` answers that where
/// `1288490188` does not. Nothing measured prints as nothing.
fn held(bytes: Option<i64>) -> String {
    let Some(bytes) = bytes else {
        return String::new();
    };
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut size = bytes as f64 / 1024.0;
    if size < 1.0 {
        return format!("{bytes} B");
    }
    for unit in UNITS {
        if size < 1024.0 || unit == "TB" {
            return format!("{size:.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{bytes} B")
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

#[cfg(test)]
mod tests {
    use super::{held, why};
    use dex_protocol::repo::KeptBecause;

    #[test]
    fn what_a_worktree_holds_is_printed_in_the_largest_unit_that_fits() {
        assert_eq!(held(Some(0)), "0 B");
        assert_eq!(held(Some(512)), "512 B");
        assert_eq!(held(Some(2048)), "2.0 KB");
        assert_eq!(held(Some(5 * 1024 * 1024)), "5.0 MB");
        assert_eq!(held(Some(3_221_225_472)), "3.0 GB");
        assert_eq!(held(Some(2 * 1024_i64.pow(4))), "2.0 TB");
    }

    #[test]
    fn a_worktree_that_was_not_measured_prints_nothing() {
        assert_eq!(held(None), "");
    }

    #[test]
    fn every_reason_a_worktree_stays_reads_as_a_reason() {
        for because in [
            KeptBecause::InUse,
            KeptBecause::Uncommitted,
            KeptBecause::Unmerged,
            KeptBecause::Detached,
            KeptBecause::Unknown,
            KeptBecause::Refused,
        ] {
            let said = why(because);
            assert!(!said.is_empty(), "{because:?} says nothing");
            assert!(
                said.chars().next().is_some_and(char::is_lowercase),
                "{said:?} reads as part of a row, not a sentence"
            );
        }
    }
}
