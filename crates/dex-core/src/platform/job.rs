//! Kill-on-close Job Object, so no child process outlives the app (docs/prd.md §7.1).
//!
//! Windows does not kill child processes when their parent dies: orphaned
//! shells and agents keep running and keep file locks, which on this app
//! strands worktrees after a crash. Putting the app itself into a job with
//! `KILL_ON_JOB_CLOSE` fixes that for every descendant at once — children
//! inherit job membership, so we never need a handle to each child (which
//! `portable-pty` does not expose, and which would otherwise need `unsafe`).

use thiserror::Error;
use win32job::{ExtendedLimitInfo, Job};

/// Failure creating or joining the job.
#[derive(Debug, Error)]
pub enum JobError {
    /// The Windows job API refused.
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
    _job: Job,
}

/// Puts the current process into a new kill-on-close job. Call once at
/// startup, before spawning any child.
pub fn contain_current_process() -> Result<ProcessJob, JobError> {
    let mut info = ExtendedLimitInfo::new();
    info.limit_kill_on_job_close();
    let job = Job::create_with_limit_info(&info)?;
    job.assign_current_process()?;
    Ok(ProcessJob { _job: job })
}
