//! Choosing where a pane's shell runs.

use super::super::WorkspaceError;
use dex_protocol::workspace::{CreateWorkspaceArgs, SplitDirection, SplitPaneArgs};

use super::super::runtimes::{check, move_panes, windows_only};
use super::super::{create, split_pane, store};
use crate::app::AppState;

fn installed() -> Vec<String> {
    vec!["Ubuntu".into(), "Debian".into()]
}

#[test]
fn windows_and_an_installed_distro_are_accepted_as_written() {
    assert_eq!(check("windows", &[]).unwrap(), "windows");
    assert_eq!(check("wsl:Ubuntu", &installed()).unwrap(), "wsl:Ubuntu");
}

#[test]
fn a_distro_that_is_not_installed_is_refused_naming_the_ones_that_are() {
    match check("wsl:Fedora", &installed()) {
        Err(WorkspaceError::NoSuchDistro { distro, installed }) => {
            assert_eq!(distro, "Fedora");
            assert_eq!(installed, vec!["Ubuntu", "Debian"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn something_that_is_not_a_runtime_is_refused() {
    assert!(matches!(
        check("linux", &installed()),
        Err(WorkspaceError::InvalidRuntime(_))
    ));
}

#[test]
fn a_distro_is_found_whatever_its_case_and_named_as_wsl_names_it() {
    assert_eq!(check("wsl:ubuntu", &installed()).unwrap(), "wsl:Ubuntu");
}

#[test]
fn a_worktree_made_by_windows_git_is_told_from_one_linux_can_read() {
    assert!(windows_only(Some(
        "gitdir: C:/src/api/.git/worktrees/fix
"
    )));
    assert!(windows_only(Some("gitdir: d:/repo/.git/worktrees/x")));
    // Made with --relative-paths, or by Linux git: readable from both sides.
    assert!(!windows_only(Some("gitdir: ../repo/.git/worktrees/x")));
    assert!(!windows_only(Some(
        "gitdir: /mnt/c/src/api/.git/worktrees/fix"
    )));
    // An ordinary checkout, whose .git is a folder, or no git at all.
    assert!(!windows_only(None));
}

#[tokio::test]
async fn moving_to_wsl_leaves_a_pane_in_a_windows_made_worktree_where_it_is() {
    let (_dir, state) = AppState::for_tests();
    let work = tempfile::tempdir().unwrap();
    let plain = work.path().join("plain");
    let worktree = work.path().join("worktree");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(
        worktree.join(".git"),
        "gitdir: C:/src/api/.git/worktrees/fix
",
    )
    .unwrap();

    let list = create(
        &state,
        CreateWorkspaceArgs {
            root_path: Some(plain.to_string_lossy().into_owned()),
            ..CreateWorkspaceArgs::default()
        },
    )
    .await
    .unwrap();
    let first = list.workspaces[0].panes[0].id.clone();
    let list = split_pane(
        &state,
        SplitPaneArgs {
            pane: first.clone(),
            direction: SplitDirection::Right,
            cwd: Some(worktree.to_string_lossy().into_owned()),
            label: None,
            kind: None,
        },
    )
    .await
    .unwrap();
    let second = list.workspaces[0]
        .panes
        .iter()
        .find(|pane| pane.id != first)
        .unwrap()
        .id
        .clone();

    assert_eq!(
        move_panes(&state, "wsl:Ubuntu".into(), false)
            .await
            .unwrap(),
        1
    );
    let runtime = |pane: String| {
        let state = state.clone();
        async move {
            state
                .db
                .call(move |conn| store::pane_runtime(conn, &pane))
                .await
                .unwrap()
                .unwrap()
        }
    };
    assert_eq!(runtime(first).await, "wsl:Ubuntu");
    assert_eq!(runtime(second).await, "windows");
}
