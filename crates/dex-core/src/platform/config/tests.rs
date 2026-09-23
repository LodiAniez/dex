use std::io::Write;
use std::time::Instant;

use super::*;
use crate::platform::bus::Bus;

fn parse(text: &str) -> (Config, Vec<String>) {
    Config::parse(text).expect("valid toml")
}

#[test]
fn an_empty_file_is_the_defaults() {
    let (config, problems) = parse("");
    assert_eq!(config, Config::default());
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_partial_file_leaves_everything_else_alone() {
    // The common case: someone sets one thing. Every other value has to survive.
    let (config, problems) = parse("[agents]\nmax_concurrent = 3\n");
    assert_eq!(config.agents.max_concurrent, 3);
    assert_eq!(config.agents.max_depth, Config::default().agents.max_depth);
    assert_eq!(config.digest, Config::default().digest);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_misspelled_key_is_reported_rather_than_ignored() {
    // Silently ignoring it would leave the owner believing a setting applied.
    let err = Config::parse("[agents]\nmax_concurent = 3\n").unwrap_err();
    assert!(err.to_string().contains("max_concurent"), "{err}");
}

#[test]
fn a_quiet_threshold_too_low_or_too_high_falls_back_to_the_default() {
    // Under a minute, every ordinary tool call would be called quiet; over a
    // day it could never be said at all, and a number large enough to overflow
    // the millis it becomes would turn the rule inside out.
    for text in [
        "[agents]\nquiet_after_seconds = 30\n",
        "[agents]\nquiet_after_seconds = 9300000000000000\n",
    ] {
        let (config, problems) = parse(text);
        assert_eq!(
            config.agents.quiet_after_seconds,
            Config::default().agents.quiet_after_seconds
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("quiet_after_seconds"), "{problems:?}");
    }
}

#[test]
fn crossed_watermarks_fall_back_to_the_defaults() {
    // low above high would pause the PTY at the high mark and never resume.
    let (config, problems) = parse("[flow]\nhigh_bytes = 1000\nlow_bytes = 2000\n");
    assert_eq!(config.flow, FlowSettings::default());
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("low_bytes"), "{problems:?}");
}

#[test]
fn a_zero_high_watermark_is_refused() {
    let (config, problems) = parse("[flow]\nhigh_bytes = 0\nlow_bytes = 0\n");
    assert_eq!(config.flow, FlowSettings::default());
    assert_eq!(problems.len(), 1);
}

#[test]
fn spawn_limits_below_one_are_raised_to_one() {
    let (config, problems) = parse("[agents]\nmax_depth = 0\nmax_concurrent = -4\n");
    assert_eq!(config.agents.max_depth, 1);
    assert_eq!(config.agents.max_concurrent, 1);
    assert_eq!(problems.len(), 2);
}

#[test]
fn an_unknown_permission_mode_falls_back_and_says_so() {
    let (config, problems) = parse("[agents]\nspawn_permission_mode = \"yolo\"\n");
    assert_eq!(config.agents.spawn_permission_mode, "auto");
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("yolo"), "{problems:?}");
}

#[test]
fn bypass_permissions_is_an_accepted_mode() {
    // Allowed as a value (PRD §9.4); whether it is honoured is the spawn's call.
    let (config, problems) = parse("[agents]\nspawn_permission_mode = \"bypassPermissions\"\n");
    assert_eq!(config.agents.spawn_permission_mode, "bypassPermissions");
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_digest_budget_too_small_to_say_anything_is_refused() {
    let (config, problems) = parse("[digest]\nfull_chars = 10\ndelta_chars = 10\n");
    assert_eq!(config.digest, DigestSettings::default());
    assert_eq!(problems.len(), 1);
}

#[test]
fn keybindings_are_overrides_only() {
    // The defaults belong to the frontend; this table says what changed.
    let (config, _) = parse("[keys]\n\"command-palette\" = \"Ctrl+Alt+P\"\n");
    assert_eq!(config.keys.len(), 1);
    assert_eq!(
        config.keys.get("command-palette").map(String::as_str),
        Some("Ctrl+Alt+P")
    );
}

#[test]
fn a_missing_file_means_defaults_and_no_complaint() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, problems) = ConfigHandle::load(dir.path().join("config.toml"));
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(*handle.get(), Config::default());
}

#[test]
fn what_was_wrong_at_startup_is_still_answerable_later() {
    // The app is a GUI: a problem only logged is a problem nobody sees. It has
    // to still be there when the owner runs `dex config show`.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[agents]\nspawn_permission_mode = \"yolo\"\n").unwrap();
    let (handle, problems) = ConfigHandle::load(path);
    assert_eq!(problems.len(), 1);
    assert_eq!(handle.problems(), problems);
}

#[test]
fn a_reload_clears_a_problem_that_was_fixed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[agents]\nmax_depth = 0\n").unwrap();
    let (handle, _) = ConfigHandle::load(path.clone());
    assert_eq!(handle.problems().len(), 1);

    std::fs::write(&path, "[agents]\nmax_depth = 3\n").unwrap();
    handle.reload();
    assert!(handle.problems().is_empty(), "{:?}", handle.problems());
}

#[test]
fn a_reload_picks_up_an_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[agents]\nmax_concurrent = 3\n").unwrap();
    let (handle, _) = ConfigHandle::load(path.clone());
    assert_eq!(handle.get().agents.max_concurrent, 3);

    std::fs::write(&path, "[agents]\nmax_concurrent = 5\n").unwrap();
    assert!(handle.reload().is_empty());
    assert_eq!(handle.get().agents.max_concurrent, 5);
}

#[test]
fn a_setting_removed_from_the_file_goes_back_to_its_default() {
    // A reload replaces the settings; it does not merge onto what was there,
    // which would make a deleted line look like it was never written.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[keys]\n\"toggle-zoom\" = \"Ctrl+Shift+Z\"\n").unwrap();
    let (handle, _) = ConfigHandle::load(path.clone());
    assert_eq!(handle.get().keys.len(), 1);

    std::fs::write(&path, "").unwrap();
    handle.reload();
    assert!(handle.get().keys.is_empty());
}

#[test]
fn a_half_written_file_leaves_the_running_settings_alone() {
    // An editor saving mid-keystroke must not reconfigure the app.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[agents]\nmax_concurrent = 3\n").unwrap();
    let (handle, _) = ConfigHandle::load(path.clone());

    std::fs::write(&path, "[agents\nmax_concurrent =").unwrap();
    let problems = handle.reload();
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("unchanged"), "{problems:?}");
    assert_eq!(handle.get().agents.max_concurrent, 3);
}

#[test]
fn the_watcher_reloads_a_saved_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[agents]\nmax_concurrent = 3\n").unwrap();
    let (handle, _) = ConfigHandle::load(path.clone());
    let bus = Bus::new();
    let mut changes = bus.subscribe();
    let _watcher = handle.watch(bus).expect("watcher starts");

    // Rename-over-target, the way an editor saves — the case a watch on the
    // file itself would miss.
    let temp = dir.path().join("config.toml.tmp");
    let mut file = std::fs::File::create(&temp).unwrap();
    file.write_all(b"[agents]\nmax_concurrent = 9\n").unwrap();
    file.sync_all().unwrap();
    drop(file);
    std::fs::rename(&temp, &path).unwrap();

    // Wait for the announcement, not the value: the watcher stores the new
    // settings and only then publishes, so a test that saw the value and asked
    // for the message at once would sometimes ask too soon.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut announced = changes.try_recv();
    while announced.is_err() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        announced = changes.try_recv();
    }
    assert_eq!(announced.map(|c| c.topic), Ok("config"));
    assert_eq!(handle.get().agents.max_concurrent, 9);
}

#[test]
fn the_update_check_is_on_by_default_and_can_be_turned_off() {
    assert!(Config::default().updates.check);
    let (config, problems) = parse("[updates]\ncheck = false\n");
    assert!(!config.updates.check);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_workspace_opens_as_terminals_unless_the_owner_prefers_another_view() {
    assert_eq!(Config::default().ui.view, "terminal");
    for view in ["terminal", "office"] {
        let (config, problems) = parse(&format!("[ui]\nview = \"{view}\"\n"));
        assert_eq!(config.ui.view, view);
        assert!(problems.is_empty(), "{problems:?}");
    }
}

#[test]
fn a_config_that_still_asks_for_the_cards_view_gets_the_office_and_is_told() {
    // 0.2.0 had a cards view. A config written for it must not read as a typo.
    let (config, problems) = parse(
        "[ui]
view = \"cards\"
",
    );
    assert_eq!(config.ui.view, "office");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("cards view was removed"),
        "{problems:?}"
    );
}

#[test]
fn a_view_that_does_not_exist_falls_back_and_says_so() {
    let (config, problems) = parse("[ui]\nview = \"penthouse\"\n");
    assert_eq!(config.ui.view, "terminal");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("ui.view"), "{problems:?}");
    assert!(problems[0].contains("penthouse"), "{problems:?}");
}

#[test]
fn spawned_agents_stay_out_of_the_browser_unless_the_owner_says_otherwise() {
    assert!(!Config::default().agents.spawn_chrome);
    let (config, problems) = parse("[agents]\nspawn_chrome = true\n");
    assert!(config.agents.spawn_chrome);
    assert!(problems.is_empty(), "{problems:?}");
}
