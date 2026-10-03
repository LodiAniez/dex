//! Panes running in another terminal: marked busy unless Dex can rule it out,
//! and recorded where their shell really starts.

use std::collections::HashMap;

use dex_protocol::pane::PaneStartedArgs;
use dex_protocol::workspace::CreateWorkspaceArgs;

use super::super::switching::{busy, running_under};
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

/// `running_under` against the real process table, with a real child.
///
/// The three-valued answer is what the agent watchdog needs: it keeps an agent
/// that is inside a tool call from being called "not responding" (issue #81),
/// while `busy` - the terminal switch's question - folds "cannot tell" into
/// "leave it alone".
#[test]
fn a_shell_with_something_under_it_is_running_under_and_an_absent_one_cannot_be_told() {
    let mut child = if cfg!(windows) {
        std::process::Command::new("cmd.exe")
            .args(["/c", "pause"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("a child to look at")
    } else {
        std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30; :"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("a child to look at")
    };
    // This process is now the parent of that child, which is all the rule
    // asks about - no PTY and no shell of our own needed.
    let me = std::process::id();
    let procs = crate::platform::proctree::snapshot().expect("the process table");

    assert_eq!(
        running_under("pane", me, "windows", Some(&procs), None),
        Some(true),
        "something is running under this very process"
    );
    assert_eq!(
        running_under("pane", 1, "windows", None, None),
        None,
        "with no process table, nothing can be told"
    );
    assert_eq!(
        running_under("pane", u32::MAX, "windows", Some(&procs), None),
        None,
        "nor about a shell that is not in it"
    );
    // And the two defaults differ: the switch errs towards busy.
    assert!(busy("pane", u32::MAX, "windows", Some(&procs), None));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_wsl_pane_is_judged_by_how_many_processes_carry_its_id() {
    let mut counts = HashMap::new();
    counts.insert("busy-pane".to_owned(), 4);
    counts.insert("idle-pane".to_owned(), 1);

    assert_eq!(
        running_under("busy-pane", 10, "wsl:Ubuntu", None, Some(&counts)),
        Some(true)
    );
    assert_eq!(
        running_under("idle-pane", 10, "wsl:Ubuntu", None, Some(&counts)),
        Some(false),
        "the shell alone"
    );
    assert_eq!(
        running_under("unknown-pane", 10, "wsl:Ubuntu", None, Some(&counts)),
        None,
        "a pane the distro did not mention"
    );
    assert_eq!(
        running_under("busy-pane", 10, "wsl:Ubuntu", None, None),
        None,
        "and a distro that did not answer at all"
    );
}
