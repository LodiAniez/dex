//! The disk projection of the context store (docs/prd.md §10.1).
//!
//! Entries are mirrored to `<root>/.dex/context/<key>.md` and the log to
//! `.dex/activity.log`, so a human can read and hand-edit workspace state and
//! an agent can find it with a plain file read when MCP is unavailable.
//!
//! SQLite is authoritative and the files are a projection, so a mirror failure
//! never fails the command that caused it — a workspace root on a disconnected
//! share or a read-only checkout must not make the store unwritable. Failures
//! go to the log.
//!
//! **Nothing here is written while the database connection is held** (issue
//! #82). Dex has one connection behind one mutex, and these writes were inside
//! the `db.call` closures that produced them - so a note on a slow disk, or a
//! workspace root on a distro's side of `\\wsl.localhost`, stopped every other
//! command in Dex until it finished, typing into a pane included: that resolves
//! the pane in the database first. A closure hands back what is `Owed` instead
//! and `flush` writes it afterwards. The two writers are private to make that
//! the only way: `flush` is async and takes no connection, so the compiler
//! refuses to put either of them back under the lock.

use std::fs;
use std::io::Write;
use std::path::Path;

use super::logic;
use super::model::Event;

/// What the mirror owes once the database call that caused it has returned.
///
/// Built inside the closure, where the workspace root and the rows are known,
/// and spent by `flush` outside it.
#[derive(Debug, Default)]
pub(super) struct Owed {
    /// The workspace root the files go under. `None` when nothing is owed.
    root: Option<String>,
    /// An entry's file: its key and the value to write.
    entry: Option<(String, String)>,
    /// A line for the activity log.
    event: Option<Event>,
}

impl Owed {
    /// What is owed under this workspace root.
    pub(super) fn at(root: &Path) -> Self {
        Self {
            root: Some(root.to_string_lossy().into_owned()),
            ..Self::default()
        }
    }

    /// Also write this entry's file.
    pub(super) fn with_entry(mut self, key: &str, value: &str) -> Self {
        self.entry = Some((key.to_owned(), value.to_owned()));
        self
    }

    /// Also append this event to the activity log.
    pub(super) fn with_event(mut self, event: Event) -> Self {
        self.event = Some(event);
        self
    }
}

/// Writes what is owed, on the blocking pool, after the database call that
/// produced it has returned.
///
/// Awaited rather than left to run on its own: the caller is about to answer,
/// and an agent that reads the file straight after a write it was told
/// succeeded should find it there. The database connection is free throughout.
pub(super) async fn flush(owed: Owed) {
    let Some(root) = owed.root else {
        return;
    };
    if owed.entry.is_none() && owed.event.is_none() {
        return;
    }
    let written = tokio::task::spawn_blocking(move || {
        let root = Path::new(&root);
        if let Some((key, value)) = owed.entry {
            write_entry(root, &key, &value);
        }
        if let Some(event) = owed.event {
            append_event(root, &event);
        }
    })
    .await;
    if let Err(err) = written {
        tracing::warn!(%err, "the activity mirror was not written");
    }
}

/// Writes one entry's file, creating its namespace directories.
fn write_entry(root: &Path, key: &str, value: &str) {
    let path = logic::mirror_path(root, key);
    let done = path
        .parent()
        .map(fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|()| fs::write(&path, value));
    report(&done, &path);
}

/// Appends one line to the activity log.
fn append_event(root: &Path, event: &Event) {
    let path = logic::activity_path(root);
    let line = format!(
        "{}\t{}\t{}\t{}\n",
        event.created_at,
        event.agent_id.as_deref().unwrap_or("human"),
        event.kind,
        event.body.replace(['\n', '\t'], " "),
    );
    let done = path
        .parent()
        .map(fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|()| fs::OpenOptions::new().create(true).append(true).open(&path))
        .and_then(|mut file| file.write_all(line.as_bytes()));
    report(&done, &path);
}

fn report<T>(done: &std::io::Result<T>, path: &Path) {
    if let Err(err) = done {
        eprintln!("dex: could not mirror {}: {err}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: &str, body: &str) -> Event {
        Event {
            seq: 1,
            workspace_id: "w1".into(),
            agent_id: Some("a1".into()),
            kind: kind.into(),
            key: None,
            body: body.into(),
            target_agent: None,
            read_at: None,
            created_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn an_entry_becomes_a_file_under_its_namespace() {
        let dir = tempfile::tempdir().unwrap();
        write_entry(dir.path(), "auth/jwt-expiry", "15 minutes");
        let path = dir.path().join(".dex/context/auth/jwt-expiry.md");
        assert_eq!(fs::read_to_string(path).unwrap(), "15 minutes");
    }

    #[test]
    fn rewriting_an_entry_replaces_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write_entry(dir.path(), "notes", "first");
        write_entry(dir.path(), "notes", "second");
        let path = dir.path().join(".dex/context/notes.md");
        assert_eq!(fs::read_to_string(path).unwrap(), "second");
    }

    #[test]
    fn events_append_one_flat_line_each() {
        let dir = tempfile::tempdir().unwrap();
        append_event(dir.path(), &event("note", "started the refactor"));
        append_event(dir.path(), &event("note", "two\nlines\tapart"));
        let log = fs::read_to_string(dir.path().join(".dex/activity.log")).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("\tnote\tstarted the refactor"));
        assert!(
            lines[1].ends_with("\tnote\ttwo lines apart"),
            "newlines and tabs would break the one-line-per-event format: {:?}",
            lines[1]
        );
    }

    #[test]
    fn an_unwritable_root_does_not_panic() {
        // The store stays usable when the mirror cannot be written.
        write_entry(Path::new("Z:/no-such-drive"), "notes", "value");
        append_event(Path::new("Z:/no-such-drive"), &event("note", "body"));
    }
}
