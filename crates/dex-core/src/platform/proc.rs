//! Running `git.exe` (docs/prd.md §8).
//!
//! **Raw git stderr never reaches the user.** Git's errors are written for
//! people who already know git; a coding agent reading "fatal: invalid
//! reference" learns nothing it can act on. Every failure here becomes a
//! `GitError` that the repo slice turns into a message and a repair. The raw
//! text is kept on the error for the log, not for display.

use std::path::Path;
use std::process::Command;

use thiserror::Error;

/// Why a git command failed.
#[derive(Debug, Clone, Error)]
pub enum GitError {
    /// `git.exe` is not installed or not on PATH.
    #[error("git is not on PATH")]
    Missing,
    /// The directory is not inside a git repository.
    #[error("{0} is not a git repository")]
    NotARepo(String),
    /// A branch, worktree, or path that must not already exist does.
    #[error("{0} already exists")]
    Exists(String),
    /// A branch or ref git could not resolve.
    #[error("git does not know {0}")]
    NoSuchRef(String),
    /// The worktree has uncommitted work, or git is otherwise holding it.
    #[error("that worktree has changes git will not discard")]
    Dirty,
    /// Anything else. Carries git's own words for the log only.
    #[error("git failed: {0}")]
    Other(String),
}

/// What a successful git command printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Trimmed stdout.
    pub stdout: String,
}

/// Runs `git` in `dir` and returns its stdout, or a translated error.
///
/// Blocking: callers are on the database's blocking pool or a spawned task.
pub fn git(dir: &Path, args: &[&str]) -> Result<Output, GitError> {
    let mut command = Command::new("git");
    command.args(args).current_dir(dir);
    // A Mac app started from the Finder has a bare PATH (`login_env.rs`).
    if let Some(path) = super::login_env::login_path() {
        command.env("PATH", path);
    }
    let output = command.output().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => GitError::Missing,
        _ => GitError::Other(err.to_string()),
    })?;
    if output.status.success() {
        return Ok(Output {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        });
    }
    Err(translate(&String::from_utf8_lossy(&output.stderr)))
}

/// Turns git's stderr into something a caller can act on.
///
/// Matching on message text is fragile in general, but git's wording for these
/// cases has been stable for years and the fallback keeps the original, so a
/// reworded message degrades to `Other` rather than to a wrong diagnosis.
pub fn translate(stderr: &str) -> GitError {
    let text = stderr.trim();
    let lower = text.to_ascii_lowercase();
    if lower.contains("not a git repository") {
        return GitError::NotARepo(subject(text).unwrap_or_else(|| "that directory".into()));
    }
    if lower.contains("already exists") || lower.contains("is already checked out") {
        return GitError::Exists(subject(text).unwrap_or_else(|| "it".into()));
    }
    if lower.contains("not a valid ref")
        || lower.contains("unknown revision")
        || lower.contains("invalid reference")
        || lower.contains("did not match any")
    {
        return GitError::NoSuchRef(subject(text).unwrap_or_else(|| "that branch".into()));
    }
    if lower.contains("contains modified or untracked files") || lower.contains("is dirty") {
        return GitError::Dirty;
    }
    GitError::Other(text.to_owned())
}

/// The quoted name in a git message, which is usually the thing at fault.
fn subject(text: &str) -> Option<String> {
    let start = text.find('\'')?;
    let rest = &text[start + 1..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_repository_is_recognised_and_names_the_path() {
        let err = translate("fatal: not a git repository (or any of the parent directories): .git");
        assert!(matches!(err, GitError::NotARepo(_)));
    }

    #[test]
    fn an_existing_branch_or_worktree_is_recognised() {
        assert!(matches!(
            translate("fatal: a branch named 'feature/x' already exists"),
            GitError::Exists(ref name) if name == "feature/x"
        ));
        assert!(matches!(
            translate("fatal: 'main' is already checked out at 'C:/repo'"),
            GitError::Exists(_)
        ));
    }

    #[test]
    fn an_unknown_branch_is_recognised() {
        assert!(matches!(
            translate("fatal: invalid reference: 'nope'"),
            GitError::NoSuchRef(ref name) if name == "nope"
        ));
        assert!(matches!(
            translate("error: pathspec 'nope' did not match any file(s) known to git"),
            GitError::NoSuchRef(_)
        ));
    }

    #[test]
    fn a_worktree_with_work_in_it_is_recognised() {
        assert!(matches!(
            translate("fatal: 'wt' contains modified or untracked files, use --force to delete it"),
            GitError::Dirty
        ));
    }

    #[test]
    fn anything_unrecognised_keeps_gits_own_words_for_the_log() {
        let err = translate("fatal: something entirely new");
        assert!(
            matches!(err, GitError::Other(ref text) if text.contains("something entirely new")),
            "an unfamiliar message must not be diagnosed as one of the known cases"
        );
    }

    #[test]
    fn a_message_with_no_quoted_name_still_classifies() {
        assert!(matches!(
            translate("fatal: not a git repository"),
            GitError::NotARepo(_)
        ));
    }
}
