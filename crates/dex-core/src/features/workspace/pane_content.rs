//! `pane.content`: the file a `markdown` pane shows (docs/prd.md §13).
//!
//! Reading is tied to a pane on purpose. The daemon could offer "read any
//! file", but then every client holding the token could; this way a file is
//! readable over the pipe only once someone has opened a pane on it, which is
//! the same person who could open it in an editor.

use std::path::Path;

use dex_protocol::pane::{PaneContent, PaneContentArgs};

use super::model::WorkspaceError;
use super::targets::resolve_pane;
use crate::app::AppState;

/// The most of a file the pane will show. A markdown pane is for notes and
/// the context mirror, not for a log; past this the reader is told it was cut.
const MAX_BYTES: usize = 1024 * 1024;

/// `pane.content`: the text of the file a `markdown` pane points at.
pub async fn content(
    state: &AppState,
    args: PaneContentArgs,
) -> Result<PaneContent, WorkspaceError> {
    let pane = state
        .db
        .call(move |conn| resolve_pane(conn, &args.pane))
        .await??;
    if pane.kind != "markdown" {
        return Err(WorkspaceError::NotMarkdown(pane.id));
    }
    // A file read can stall on a slow disk or a network share; keep it off the
    // async threads like every other blocking call.
    tokio::task::spawn_blocking(move || read(&pane.cwd))
        .await
        .map_err(|err| WorkspaceError::Unreadable {
            path: String::new(),
            reason: err.to_string(),
        })?
}

fn read(path: &str) -> Result<PaneContent, WorkspaceError> {
    let unreadable = |err: std::io::Error| WorkspaceError::Unreadable {
        path: path.to_owned(),
        reason: err.to_string(),
    };
    let native = Path::new(path);
    let bytes = std::fs::read(native).map_err(unreadable)?;
    let modified_at = std::fs::metadata(native)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0);
    let truncated = bytes.len() > MAX_BYTES;
    let kept = if truncated {
        &bytes[..MAX_BYTES]
    } else {
        &bytes[..]
    };
    Ok(PaneContent {
        path: path.to_owned(),
        // Lossy on purpose: a stray byte in a notes file should not make the
        // whole pane an error.
        text: String::from_utf8_lossy(kept).into_owned(),
        truncated,
        modified_at,
    })
}
