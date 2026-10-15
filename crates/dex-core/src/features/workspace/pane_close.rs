//! `pane.close`: mending the tree the pane was in, and ending what it left
//! running.
//!
//! The second half is why this is not in `pane_commands.rs`. Dex kills the
//! Windows side of a pane - its PTY, and `wsl.exe` for a pane in a distro -
//! and a distro keeps its own processes: an agent's command outlived its pane
//! by three minutes, then by hours, and had to be killed by hand from another
//! terminal (issue #68).

use dex_protocol::workspace::{PaneArgs, WorkspaceList};

use super::commands::load_list;
use super::layout::{self, Closed};
use super::model::{Pane, WorkspaceError};
use super::pane_commands::{locate, to_json};
use super::store;
use crate::app::AppState;
use crate::platform::clock;
use crate::platform::wsl::{self, Runtime};

/// The kind of pane that runs a shell, and so can leave anything behind.
const TERMINAL: &str = "terminal";

/// What `pane.close` answers with: the list, and what the closed pane left
/// running inside a distro, if that is where it was.
type ClosedPane = rusqlite::Result<Result<(WorkspaceList, Option<InDistro>), WorkspaceError>>;

/// A closed pane whose processes are a distro's, not Windows'.
struct InDistro {
    distro: String,
    pane: String,
}

/// `pane.close`: removes a pane; its sibling takes the parent split's place.
/// A workspace keeps at least one pane. If the closed pane had focus, focus
/// moves to the first pane left.
pub async fn close_pane(state: &AppState, args: PaneArgs) -> Result<WorkspaceList, WorkspaceError> {
    let now = clock::now_millis();
    let (list, left) = state
        .db
        .call(move |conn| -> ClosedPane {
            let found = match locate(conn, &args.pane)? {
                Ok(found) => found,
                Err(err) => return Ok(Err(err)),
            };
            // Read before the row goes, and acted on after the connection is
            // free: a `wsl.exe` run takes a second at best and the database is
            // one connection behind one lock (conventions 4.4).
            let in_distro = left_in_distro(&found.pane);
            let tree = match layout::close_pane(&found.layout, &found.pane.id) {
                Closed::Remaining(tree) => tree,
                Closed::WasLast => return Ok(Err(WorkspaceError::LastPane)),
                Closed::NotFound => return Ok(Err(WorkspaceError::NoSuchPane(args.pane))),
            };
            let focus = if found.active.as_deref() == Some(found.pane.id.as_str()) {
                layout::leaves(&tree).into_iter().next()
            } else {
                found.active
            };
            let json = match to_json(&tree) {
                Ok(json) => json,
                Err(err) => return Ok(Err(err)),
            };
            let tx = conn.transaction()?;
            store::delete_pane(&tx, &found.pane.id)?;
            store::update_layout(&tx, &found.pane.workspace_id, &json, focus.as_deref(), now)?;
            tx.commit()?;
            load_list(conn).map(|list| Ok((list, in_distro)))
        })
        .await??;
    if let Some(left) = left {
        end_in_distro(left);
    }
    Ok(list)
}

/// What a closed pane leaves running in a distro: nothing, unless its shell
/// ran in one. Pure.
///
/// A pane of any other kind has no shell and so started nothing; a runtime
/// that cannot be read names no distro to ask.
fn left_in_distro(pane: &Pane) -> Option<InDistro> {
    match Runtime::parse(&pane.runtime) {
        Ok(Runtime::Wsl(distro)) if pane.kind == TERMINAL => Some(InDistro {
            distro,
            pane: pane.id.clone(),
        }),
        _ => None,
    }
}

/// Ends a closed pane's processes inside its distro, in the background.
///
/// Not awaited: the pane is gone either way, the answer is the new tree, and
/// a `wsl.exe` run can take seconds - closing a pane must stay instant. The
/// result is logged, since nothing is left to tell.
fn end_in_distro(left: InDistro) {
    tokio::spawn(async move {
        let InDistro { distro, pane } = left;
        let (asked, named) = (distro.clone(), pane.clone());
        let ended =
            tokio::task::spawn_blocking(move || wsl::end_pane_processes(&asked, &named)).await;
        match ended {
            Ok(Ok(ended)) if ended.hung_up.is_empty() => {
                tracing::debug!(%distro, %pane, "the closed pane left nothing running in the distro");
            }
            Ok(Ok(ended)) => tracing::info!(
                %distro, %pane,
                hung_up = ?ended.hung_up, killed = ?ended.killed,
                "ended what a closed pane left running in the distro"
            ),
            Ok(Err(err)) => {
                tracing::warn!(%distro, %pane, %err, "could not end what a closed pane left running in the distro");
            }
            Err(err) => {
                tracing::warn!(%distro, %pane, %err, "the task ending a closed pane's processes did not finish");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(kind: &str, runtime: &str) -> Pane {
        Pane {
            id: "p-1".into(),
            workspace_id: "w-1".into(),
            label: None,
            cwd: "C:/work".into(),
            kind: kind.into(),
            runtime: runtime.into(),
        }
    }

    #[test]
    fn only_a_shell_in_a_distro_leaves_anything_to_end_there() {
        let left = left_in_distro(&pane("terminal", "wsl:Ubuntu")).expect("a shell in a distro");
        assert_eq!(left.distro, "Ubuntu");
        assert_eq!(left.pane, "p-1");

        // Windows' own panes are in Dex's kill-on-close job; the kinds that
        // run no shell started nothing; and a runtime nobody can read names
        // no distro to ask.
        for nothing in [
            pane("terminal", "windows"),
            pane("markdown", "wsl:Ubuntu"),
            pane("diff", "wsl:Ubuntu"),
            pane("activity", "wsl:Ubuntu"),
            pane("terminal", "wsl:"),
            pane("terminal", "linux"),
        ] {
            assert!(
                left_in_distro(&nothing).is_none(),
                "{} in {}",
                nothing.kind,
                nothing.runtime
            );
        }
    }
}
