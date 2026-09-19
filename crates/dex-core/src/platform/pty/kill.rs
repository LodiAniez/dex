//! What ending a pane's process did, read the right way round.

use super::PtyError;

/// The pane's `ChildKiller::kill`, as Dex reports it.
///
/// portable-pty 0.9.0's cloned Windows killer has its check inverted: when
/// TerminateProcess *succeeds* it returns `last_os_error()` - an error left
/// on the thread by whatever failed before, of any value - and Ok when it
/// fails (ARCHITECTURE.md).
#[cfg(windows)]
pub(super) fn outcome(result: std::io::Result<()>) -> Result<(), PtyError> {
    match result {
        Err(_) => Ok(()),
        Ok(()) => Err(PtyError::KillFailed),
    }
}

/// On macOS and Linux the killer reports as it should.
#[cfg(not(windows))]
pub(super) fn outcome(result: std::io::Result<()>) -> Result<(), PtyError> {
    result.map_err(PtyError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn any_error_from_the_windows_killer_means_the_process_ended() {
        for code in [0, 2, 183] {
            let said = Err(std::io::Error::from_raw_os_error(code));
            assert!(outcome(said).is_ok(), "OS error {code}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn ok_from_the_windows_killer_means_the_process_did_not_end() {
        assert!(matches!(outcome(Ok(())), Err(PtyError::KillFailed)));
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_the_killer_is_taken_at_its_word() {
        assert!(outcome(Ok(())).is_ok());
        assert!(outcome(Err(std::io::Error::other("no such process"))).is_err());
    }
}
