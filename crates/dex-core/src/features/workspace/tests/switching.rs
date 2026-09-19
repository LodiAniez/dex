//! Panes running in another terminal: marked busy unless Dex can rule it out,
//! and recorded where their shell really starts.

use std::collections::HashMap;

use dex_protocol::pane::PaneStartedArgs;
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::super::switching::busy;
use super::super::{create, record_started, store};
use crate::app::AppState;
use crate::platform::proctree::Proc;

fn proc(pid: u32, parent: u32, name: &str) -> Proc {
    Proc {
        pid,
        parent,
        name: name.into(),
        command: name.into(),
    }
}

#[test]
fn a_windows_shell_is_plain_only_when_the_table_shows_nothing_under_it() {
    let procs = [
        proc(10, 1, "pwsh.exe"),
        proc(20, 1, "pwsh.exe"),
        proc(21, 20, "node.exe"),
    ];
    assert!(!busy("p", 10, "windows", Some(&procs), None));
    assert!(busy("p", 20, "windows", Some(&procs), None));
    // A table that could not be read, or a shell not in it, rules nothing out.
    assert!(busy("p", 10, "windows", None, None));
    assert!(busy("p", 99, "windows", Some(&procs), None));
}

#[test]
fn a_wsl_shell_is_plain_only_when_it_alone_carries_its_pane() {
    let counts = HashMap::from([("alone".to_owned(), 1), ("working".to_owned(), 3)]);
    assert!(!busy("alone", 0, "wsl:Ubuntu", None, Some(&counts)));
    assert!(busy("working", 0, "wsl:Ubuntu", None, Some(&counts)));
    assert!(busy("unknown", 0, "wsl:Ubuntu", None, Some(&counts)));
    assert!(
        busy("alone", 0, "wsl:Ubuntu", None, None),
        "the distro did not answer"
    );
}

#[tokio::test]
async fn a_pane_records_where_its_shell_really_started() {
    let (_dir, state) = AppState::for_tests();
    let list = create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let pane = list.workspaces[0].panes[0].id.clone();
    record_started(
        &state,
        PaneStartedArgs {
            pane: pane.clone(),
            runtime: "wsl:Ubuntu".into(),
        },
    )
    .await
    .unwrap();
    let runtime = state
        .db
        .call(move |conn| store::pane_runtime(conn, &pane))
        .await
        .unwrap();
    assert_eq!(runtime.as_deref(), Some("wsl:Ubuntu"));
}

#[tokio::test]
async fn a_shell_start_is_announced_only_when_it_moved_the_pane() {
    let (_dir, state) = AppState::for_tests();
    let list = create(&state, CreateWorkspaceArgs::default())
        .await
        .unwrap();
    let pane = list.workspaces[0].panes[0].id.clone();
    let mut changes = state.bus.subscribe();
    let started = |runtime: &str| PaneStartedArgs {
        pane: pane.clone(),
        runtime: runtime.into(),
    };
    // Where it already runs: nothing new to say.
    record_started(&state, started("windows")).await.unwrap();
    assert!(changes.try_recv().is_err());
    record_started(&state, started("wsl:Ubuntu")).await.unwrap();
    assert_eq!(changes.try_recv().map(|c| c.topic), Ok("workspaces"));
    // A pane since closed, or a runtime that is not one, changes nothing.
    record_started(
        &state,
        PaneStartedArgs {
            pane: "gone".into(),
            runtime: "windows".into(),
        },
    )
    .await
    .unwrap();
    assert!(changes.try_recv().is_err());
    assert!(record_started(&state, started("linux")).await.is_err());
}
