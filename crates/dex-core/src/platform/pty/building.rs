//! Turning a `SpawnRequest` into what `portable-pty` wants, and the two
//! one-liners every path in the supervisor uses.
//!
//! Here rather than in `pty.rs` because that file is at the 400-line ceiling
//! (docs/conventions.md §6.3) and these four are the part of it that is not about
//! owning a pane: given a request they answer with a command, a size, an
//! error, or a named thread, and they hold nothing.

use std::thread;

use portable_pty::{CommandBuilder, PtySize};

use super::session_env::forget_claude_session;
use super::{PtyError, SpawnRequest};

pub(super) fn command(request: &SpawnRequest) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(&request.program);
    cmd.args(&request.args);
    cmd.cwd(&request.cwd);
    forget_claude_session(&mut cmd);
    for (key, value) in &request.env {
        cmd.env(key, value);
    }
    cmd
}

pub(super) fn cell_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub(super) fn pty_err(err: impl std::fmt::Display) -> PtyError {
    PtyError::Pty(err.to_string())
}

pub(super) fn spawn_named(
    name: String,
    body: impl FnOnce() + Send + 'static,
) -> Result<(), PtyError> {
    thread::Builder::new().name(name).spawn(body)?;
    Ok(())
}
