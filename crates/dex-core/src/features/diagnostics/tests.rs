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

#[test]
fn the_view_carries_what_the_office_pane_needs_as_values() {
    // The effective settings are text for a human; a client must not parse
    // them to learn how many seats there are.
    use crate::platform::config::{Config, ConfigHandle};

    let (_dir, mut state) = AppState::for_tests();
    let mut config = Config::default();
    config.agents.max_concurrent = 9;
    config.ui.office_view = "office".into();
    state.config = ConfigHandle::fixed(config);

    let view = get_config(&state).expect("printable");
    assert_eq!(view.max_concurrent, 9);
    assert_eq!(view.office_view, "office");
}
