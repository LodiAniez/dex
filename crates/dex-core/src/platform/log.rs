//! Where the daemon's own words go (issue #73).
//!
//! They went to stdout, which a windowed app does not have: every
//! `tracing::warn!` in the daemon was written to a handle nobody holds. So when
//! the owner asked to see Dex's logs after an hour of the app not responding,
//! there was nothing to read, and the answer had to be pieced together from the
//! database, the workspace's activity mirror and the Windows process table.
//!
//! A file in the data directory instead, beside `dex.db`: one `dex.log`, one
//! `dex.log.1` behind it, and a cap on each so a long-running daemon cannot
//! fill a disk that - as of this week - is known to be the thing the owner runs
//! out of. Rotation is by size and happens in the writer, because the
//! alternative is a timer and a daemon that may be asleep when it fires.
//!
//! No new dependency: `tracing_subscriber`'s `MakeWriter` takes any `Fn() ->
//! impl Write`, so a handle that can be cloned is all the wiring the app needs
//! (`dex_core::platform::log::open`).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// How large `dex.log` grows before it becomes `dex.log.1`.
const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// A handle on the log file. Cheap to clone; every clone writes to the same
/// file under the same lock.
#[derive(Clone)]
pub struct Log {
    inner: Arc<Mutex<Writing>>,
}

struct Writing {
    path: PathBuf,
    file: File,
    written: u64,
    max: u64,
}

/// Opens `<dir>/dex.log` for appending, rotating what is already there if it is
/// over the cap. Fails only if the file cannot be opened at all, which the
/// caller should treat as "log to stdout and carry on" rather than as fatal:
/// not being able to write a log is no reason not to start.
pub fn open(dir: &Path) -> io::Result<Log> {
    with_cap(dir, MAX_BYTES)
}

fn with_cap(dir: &Path, max: u64) -> io::Result<Log> {
    fs::create_dir_all(dir)?;
    let path = dir.join("dex.log");
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let written = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    Ok(Log {
        inner: Arc::new(Mutex::new(Writing {
            path,
            file,
            written,
            max,
        })),
    })
}

impl Write for Log {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Ok(mut writing) = self.inner.lock() else {
            // A poisoned lock means another thread panicked mid-write. Losing
            // the line is better than panicking in whatever is being logged.
            return Ok(bytes.len());
        };
        writing.roll_if_full()?;
        let wrote = writing.file.write(bytes)?;
        writing.written += wrote as u64;
        Ok(wrote)
    }

    fn flush(&mut self) -> io::Result<()> {
        let Ok(mut writing) = self.inner.lock() else {
            return Ok(());
        };
        writing.file.flush()
    }
}

impl Writing {
    /// Moves the log aside once it is over the cap, keeping one behind it.
    ///
    /// Best effort by design: if the rename fails - something has the old file
    /// open, which on Windows is normal enough - the current file keeps being
    /// written to and grows past the cap rather than losing the line.
    fn roll_if_full(&mut self) -> io::Result<()> {
        if self.written < self.max {
            return Ok(());
        }
        let behind = self.path.with_extension("log.1");
        let _ = fs::remove_file(&behind);
        if fs::rename(&self.path, &behind).is_err() {
            self.written = 0;
            return Ok(());
        }
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.written = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::with_cap;

    #[test]
    fn what_is_written_is_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = with_cap(dir.path(), 1024).unwrap();

        log.write_all(b"the daemon said something\n").unwrap();
        log.flush().unwrap();

        let written = std::fs::read_to_string(dir.path().join("dex.log")).unwrap();
        assert!(written.contains("the daemon said something"));
    }

    #[test]
    fn a_full_log_moves_aside_and_one_behind_it_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = with_cap(dir.path(), 64).unwrap();

        for line in 0..12 {
            writeln!(log, "line {line} padded out to pass the cap").unwrap();
        }
        log.flush().unwrap();

        let current = std::fs::read_to_string(dir.path().join("dex.log")).unwrap();
        let behind = std::fs::read_to_string(dir.path().join("dex.log.1")).unwrap();
        assert!(
            current.contains("line 11"),
            "the newest lines are in dex.log: {current}"
        );
        assert!(!behind.is_empty(), "and the older ones are behind it");
        assert!(
            !current.contains("line 0"),
            "which is no longer in the current file"
        );
    }

    #[test]
    fn a_log_that_is_already_there_is_appended_to_rather_than_replaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("dex.log"), "from the last run\n").unwrap();

        let mut log = with_cap(dir.path(), 1024).unwrap();
        log.write_all(b"from this one\n").unwrap();
        log.flush().unwrap();

        let written = std::fs::read_to_string(dir.path().join("dex.log")).unwrap();
        assert!(written.contains("from the last run"), "{written}");
        assert!(written.contains("from this one"), "{written}");
    }

    #[test]
    fn two_handles_write_to_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut one = with_cap(dir.path(), 1024).unwrap();
        let mut two = one.clone();

        one.write_all(b"from one\n").unwrap();
        two.write_all(b"from two\n").unwrap();
        two.flush().unwrap();

        let written = std::fs::read_to_string(dir.path().join("dex.log")).unwrap();
        assert!(
            written.contains("from one") && written.contains("from two"),
            "{written}"
        );
    }
}
