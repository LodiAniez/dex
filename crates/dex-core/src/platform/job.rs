//! No child process outlives the app (docs/prd.md §7.1).
//!
//! **Windows** does not kill child processes when their parent dies: orphaned
//! shells and agents keep running and keep file locks, which on this app
//! strands worktrees after a crash. Putting the app itself into a job with
//! `KILL_ON_JOB_CLOSE` fixes that for every descendant at once — children
//! inherit job membership, so we never need a handle to each child (which
//! `portable-pty` does not expose, and which would otherwise need `unsafe`).
//!
//! **macOS and Linux** need nothing: each pane's shell leads its own session
//! on its terminal, and when the app goes - cleanly or in a crash - its end of
//! every terminal closes, and the kernel sends each session a hangup, which
//! ends the shell and whatever runs in it. That is how every terminal app
//! ends its shells.

use thiserror::Error;

/// Failure creating or joining the job.
#[derive(Debug, Error)]
pub enum JobError {
    /// The Windows job API refused.
    #[cfg(windows)]
    #[error("job object: {0}")]
    Job(#[from] win32job::JobError),
}

/// The job containing this process and all its descendants.
///
/// **Keep it alive for the whole life of the process.** Dropping it closes the
/// last handle to the job, and Windows then kills every process in the job —
/// including this one.
#[derive(Debug)]
pub struct ProcessJob {
    #[cfg(windows)]
    _job: win32job::Job,
}

/// Puts the current process into a new kill-on-close job. Call once at
/// startup, before spawning any child. On macOS and Linux there is nothing to
/// do (see the module docs).
pub fn contain_current_process() -> Result<ProcessJob, JobError> {
    #[cfg(windows)]
    {
        let mut info = win32job::ExtendedLimitInfo::new();
        info.limit_kill_on_job_close();
        let job = win32job::Job::create_with_limit_info(&info)?;
        job.assign_current_process()?;
        Ok(ProcessJob { _job: job })
    }
    #[cfg(not(windows))]
    {
        Ok(ProcessJob {})
    }
}
