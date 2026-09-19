//! The protocol code and repair string for every error a command can produce.
//!
//! Split from `router.rs` only for size; this is the same single place where
//! no error can reach a client without an actionable `repair`
//! (docs/conventions.md §4.3).

use dex_protocol::{ErrorBody, ErrorCode};

use super::CoreError;
use crate::features::agent::AgentError;
use crate::features::context::ContextError;
use crate::features::diagnostics::DiagnosticsError;
use crate::features::repo::RepoError;
use crate::features::workspace::WorkspaceError;

const REPAIR_BUG: &str =
    "This is a bug in Dex, not in your input. The app log has the details; please report it.";

/// The protocol code and repair string for every error.
pub(super) fn error_body(err: &CoreError) -> ErrorBody {
    let (code, repair) = match err {
        CoreError::UnknownCommand(_) => (
            ErrorCode::InvalidArgs,
            "Check the command name; `dex --help` lists every command.".to_owned(),
        ),
        CoreError::InvalidArgs { .. } => (
            ErrorCode::InvalidArgs,
            "Check the arguments; `dex <command> --help` lists what the command accepts."
                .to_owned(),
        ),
        CoreError::Encode(_) => (ErrorCode::Internal, REPAIR_BUG.to_owned()),
        // Every setting was read from TOML, so failing to write it back is a bug.
        CoreError::Diagnostics(DiagnosticsError::Unprintable(_)) => {
            (ErrorCode::Internal, REPAIR_BUG.to_owned())
        }
        CoreError::Workspace(err) => workspace_repair(err),
        CoreError::Agent(AgentError::NoSuchPane(_)) => (
            ErrorCode::NoSuchPane,
            "The hook ran in a pane Dex no longer has; nothing to do.".to_owned(),
        ),
        CoreError::Agent(AgentError::NoSuchAgent(_)) => (
            ErrorCode::NoSuchAgent,
            "Run `dex agent list` to see the running agents; target one by its id or its pane's label."
                .to_owned(),
        ),
        CoreError::Agent(AgentError::NotRunning(_)) => (
            ErrorCode::NoSuchAgent,
            "That agent has already ended; `dex agent list` shows the running ones.".to_owned(),
        ),
        CoreError::Agent(AgentError::NotYours(_)) => (
            ErrorCode::InvalidArgs,
            "An agent may stop itself and the agents it spawned, nobody else's. If this one should end, tell the owner or the agent that spawned it (`message_send`).".to_owned(),
        ),
        CoreError::Agent(AgentError::EmptyPrompt) => (
            ErrorCode::InvalidArgs,
            "Say what to tell the agent.".to_owned(),
        ),
        CoreError::Agent(AgentError::NotPromptable(_)) => (
            ErrorCode::InvalidArgs,
            "Go to the agent's pane and look: typing there is only safe while Claude Code is at its prompt or working. A memo (`dex context send`) waits in its inbox instead.".to_owned(),
        ),
        CoreError::Agent(AgentError::EmptyBrief) => (
            ErrorCode::InvalidArgs,
            "Say what the new agent should do: `--task \"port the auth module\"`.".to_owned(),
        ),
        CoreError::Agent(AgentError::BriefTooLong { .. }) => (
            ErrorCode::InvalidArgs,
            "Keep --task to what the agent must do, and put the detail in shared context: `dex context write <key>`, then name the key in the brief.".to_owned(),
        ),
        CoreError::Agent(AgentError::WorktreeWithoutRepo) => (
            ErrorCode::InvalidArgs,
            "Name the repository too: `--repo <name> --worktree <branch>`.".to_owned(),
        ),
        CoreError::Agent(AgentError::NoCaller) => (
            ErrorCode::NotInPane,
            "Run this inside a Dex pane, or pass --workspace <name-or-id>.".to_owned(),
        ),
        // The repair tells the agent to do the work itself: an agent that has
        // hit the limit needs a way forward, not just a refusal (PRD §9.4).
        CoreError::Agent(AgentError::DepthLimit { max, .. }) => (
            ErrorCode::DepthLimit,
            format!(
                "Agents may be nested {max} deep, and this would be one more. Do this part of the \
                 work yourself instead of delegating it."
            ),
        ),
        CoreError::Agent(AgentError::ConcurrencyLimit { max, .. }) => (
            ErrorCode::ConcurrencyLimit,
            format!(
                "This workspace allows {max} agents at once. Wait for one to finish, stop one with \
                 `dex agent stop`, or do this part of the work yourself."
            ),
        ),
        CoreError::Agent(AgentError::NoSuchRepo(_)) => (
            ErrorCode::InvalidArgs,
            "Run `dex repo list` to see the registered repositories.".to_owned(),
        ),
        CoreError::Agent(AgentError::Repo(err)) => repo_repair(err),
        CoreError::Agent(AgentError::Target(err)) => workspace_repair(err),
        CoreError::Agent(AgentError::Db(_)) => (ErrorCode::Internal, REPAIR_BUG.to_owned()),
        CoreError::Context(err) => context_repair(err),
        CoreError::Repo(err) => repo_repair(err),
    };
    ErrorBody {
        code,
        message: err.to_string(),
        repair,
    }
}

/// Git's own words never reach a client: every failure becomes something the
/// caller can act on (PRD §8).
fn repo_repair(err: &RepoError) -> (ErrorCode, String) {
    use crate::platform::proc::GitError;
    let (code, repair) = match err {
        RepoError::NoSuchRepo(_) => (
            ErrorCode::InvalidArgs,
            "Run `dex repo list` to see the registered repositories, or add one with `dex repo add <path>`."
                .to_owned(),
        ),
        RepoError::AmbiguousRepo { candidates, .. } => {
            return (
                ErrorCode::AmbiguousTarget,
                format!("Name one of them exactly: {}.", candidates.join("; ")),
            );
        }
        RepoError::NoSuchDir(_) => (
            ErrorCode::InvalidArgs,
            "Give an existing folder, as an absolute path.".to_owned(),
        ),
        RepoError::InvalidBranch { reason, .. } => {
            (ErrorCode::InvalidArgs, reason.repair().to_owned())
        }
        RepoError::GitTooOld { place, .. } if place == "Windows" => (
            ErrorCode::InvalidArgs,
            "Install the current Git for Windows (https://git-scm.com), then spawn again.".to_owned(),
        ),
        RepoError::GitTooOld { place, .. } => (
            ErrorCode::InvalidArgs,
            format!(
                "Install or update git in {place}. On Ubuntu: `sudo add-apt-repository ppa:git-core/ppa && sudo apt update && sudo apt install git`. Then spawn again."
            ),
        ),
        RepoError::Git(GitError::Missing) => (
            ErrorCode::GitFailed,
            "Install Git for Windows and make sure git.exe is on PATH.".to_owned(),
        ),
        RepoError::Git(GitError::NotARepo(_)) => (
            ErrorCode::GitFailed,
            "Point at a folder inside a git repository, or run `git init` there first.".to_owned(),
        ),
        RepoError::Git(GitError::Exists(_)) => (
            ErrorCode::GitFailed,
            "Pick another branch name, or use the worktree that already exists.".to_owned(),
        ),
        RepoError::Git(GitError::NoSuchRef(_)) => (
            ErrorCode::GitFailed,
            "Check the branch name; `git branch -a` in the repository lists them.".to_owned(),
        ),
        RepoError::Git(GitError::Dirty) => (
            ErrorCode::GitFailed,
            "Commit or discard the work in that worktree, or remove it with --force.".to_owned(),
        ),
        RepoError::Git(GitError::Other(_)) => (
            ErrorCode::GitFailed,
            "Run the same git command in the repository to see what it reports.".to_owned(),
        ),
        RepoError::Target(err) => return workspace_repair(err),
        RepoError::Db(_) => (ErrorCode::Internal, REPAIR_BUG.to_owned()),
    };
    (code, repair)
}

fn context_repair(err: &ContextError) -> (ErrorCode, String) {
    let (code, repair) = match err {
        ContextError::InvalidKey { reason, .. } => {
            (ErrorCode::InvalidArgs, reason.repair().to_owned())
        }
        ContextError::NoSuchKey(_) => (
            ErrorCode::InvalidArgs,
            "Run `dex context list` to see the keys that exist, or write it first.".to_owned(),
        ),
        // The repair carries the current value so the caller can merge in one
        // step instead of reading, diffing, and racing again.
        ContextError::VersionConflict { current, value, .. } => {
            return (
                ErrorCode::VersionConflict,
                format!(
                    "Someone else wrote this key first. It now holds {value:?} at version \
                     {current}. Merge your change into that value and write again with \
                     expected_version={current}."
                ),
            );
        }
        ContextError::NoSuchAgent(_) => (
            ErrorCode::NoSuchAgent,
            "Run `dex agent list` to see which agents are running, and target one by label."
                .to_owned(),
        ),
        ContextError::NoWorkspace => (
            ErrorCode::NotInPane,
            "Run this inside a Dex pane, or pass --workspace <name-or-id>.".to_owned(),
        ),
        ContextError::NoSuchEvent(_) => (
            ErrorCode::InvalidArgs,
            "The activity pane shows each event's number; it may already have been removed."
                .to_owned(),
        ),
        ContextError::Target(err) => return workspace_repair(err),
        ContextError::Db(_) => (ErrorCode::Internal, REPAIR_BUG.to_owned()),
    };
    (code, repair)
}

fn workspace_repair(err: &WorkspaceError) -> (ErrorCode, String) {
    let (code, repair) = match err {
        WorkspaceError::NoSuchWorkspace(_) => (
            ErrorCode::NoSuchWorkspace,
            "Run `dex workspace list` to see the workspaces that exist.",
        ),
        WorkspaceError::NoSuchPane(_) => (
            ErrorCode::NoSuchPane,
            "Run `dex pane list` to see the panes that exist.",
        ),
        WorkspaceError::AmbiguousTarget { candidates, .. } => {
            return (
                ErrorCode::AmbiguousTarget,
                format!(
                    "Target one of them by id instead: {}.",
                    candidates.join("; ")
                ),
            );
        }
        WorkspaceError::PaneNotStarted(_) => (
            ErrorCode::NoSuchPane,
            "A pane's shell starts when the pane is first shown: switch to its workspace in Dex, then retry.",
        ),
        WorkspaceError::InvalidColor(_) => (
            ErrorCode::InvalidArgs,
            "Use a hex color like #4f8cff, or no color.",
        ),
        WorkspaceError::InvalidName => (
            ErrorCode::InvalidArgs,
            "Give the workspace a name between 1 and 64 characters.",
        ),
        WorkspaceError::InvalidOrder => (
            ErrorCode::InvalidArgs,
            "List every workspace id exactly once, in the new order.",
        ),
        WorkspaceError::InvalidRoot(_) => (
            ErrorCode::InvalidArgs,
            "Choose an existing folder, as an absolute path.",
        ),
        WorkspaceError::LastPane => (
            ErrorCode::InvalidArgs,
            "A workspace keeps at least one pane; delete the workspace to close it.",
        ),
        WorkspaceError::LayoutMismatch => (
            ErrorCode::InvalidArgs,
            "Send a layout that shows every pane of the workspace exactly once.",
        ),
        WorkspaceError::LabelTaken(_) => (
            ErrorCode::InvalidArgs,
            "Labels belong to panes, and a pane keeps its label after its agent has ended: look in `dex pane list`. Pick another label, or relabel or close that pane first.",
        ),
        WorkspaceError::InvalidLabel => (
            ErrorCode::InvalidArgs,
            "Use a short label without spaces, like `server` or `tests`.",
        ),
        WorkspaceError::InvalidRuntime(_) => (
            ErrorCode::InvalidArgs,
            "Use `windows`, or `wsl:<distro>` for a WSL distro; `wsl.exe --list` names them.",
        ),
        WorkspaceError::NoSuchDistro { installed, .. } if installed.is_empty() => (
            ErrorCode::InvalidArgs,
            "No WSL distro is installed. Install one (`wsl --install Ubuntu`), or run this on Windows.",
        ),
        WorkspaceError::NoSuchDistro { installed, .. } => {
            return (
                ErrorCode::InvalidArgs,
                format!(
                    "Use an installed distro ({}), or run this on Windows.",
                    installed.join(", ")
                ),
            );
        }
        WorkspaceError::InvalidKind(_) => (
            ErrorCode::InvalidArgs,
            "Use `terminal` for a shell, `activity` for the workspace's live event stream, `diff` for a repository's changes, or `markdown` for a file.",
        ),
        WorkspaceError::NeedsFile => (
            ErrorCode::InvalidArgs,
            "Say which file to show: `--kind markdown --path notes.md`.",
        ),
        WorkspaceError::InvalidFile(_) => (
            ErrorCode::InvalidArgs,
            "Give an existing file, as an absolute path.",
        ),
        WorkspaceError::NotMarkdown(_) => (
            ErrorCode::InvalidArgs,
            "Only a markdown pane has content to read; `dex pane list` shows each pane's kind.",
        ),
        WorkspaceError::Unreadable { .. } => (
            ErrorCode::Internal,
            "Check that the file still exists and that you can open it.",
        ),
        WorkspaceError::Pty(_) | WorkspaceError::Layout(_) | WorkspaceError::Db(_) => {
            (ErrorCode::Internal, REPAIR_BUG)
        }
    };
    (code, repair.to_owned())
}
