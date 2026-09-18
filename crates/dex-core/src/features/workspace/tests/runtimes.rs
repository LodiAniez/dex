//! Choosing where a pane's shell runs.

use super::super::WorkspaceError;
use super::super::runtimes::check;

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
