//! Store tests against a temp database and command tests against a fake AppState.

use super::*;
use crate::app::AppState;

#[test]
fn the_effective_settings_round_trip_back_to_toml() {
    // What `dex config show` prints has to be a file the owner could save back.
    let (_dir, state) = AppState::for_tests();
    let view = get_config(&state).expect("printable");
    let (parsed, problems) =
        crate::platform::config::Config::parse(&view.effective).expect("valid toml");
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(&parsed, state.config.get().as_ref());
}

#[test]
fn a_workspace_with_no_config_file_reports_it_as_absent() {
    let (_dir, state) = AppState::for_tests();
    let view = get_config(&state).expect("printable");
    assert!(!view.present);
    assert!(view.problems.is_empty());
    assert!(view.keys.is_empty(), "no overrides by default");
}
