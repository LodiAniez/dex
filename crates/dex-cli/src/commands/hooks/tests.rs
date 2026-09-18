use super::*;

const EXE: &str = r"C:\Program Files\Dex\dex.exe";

fn user_settings() -> Value {
    serde_json::from_str(
        r#"{
            "model": "opus",
            "hooks": {
                "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "audit.exe" }] }],
                "Stop": [{ "hooks": [{ "type": "command", "command": "notify.exe" }] }]
            },
            "theme": "dark"
        }"#,
    )
    .unwrap()
}

#[test]
fn install_into_an_empty_file_adds_every_hook_in_exec_form() {
    let mut doc = json!({});
    install(&mut doc, EXE).unwrap();
    assert_eq!(status(&doc, EXE).current, HOOKS.len());
    let stop = &doc["hooks"]["Stop"][0]["hooks"][0];
    assert_eq!(stop["command"], EXE);
    assert_eq!(stop["args"], json!(["event", "stop"]));
    assert_eq!(stop["async"], true);
    assert!(
        doc["hooks"]["SessionStart"][0]["hooks"][0]
            .get("async")
            .is_none()
    );
    assert_eq!(doc["hooks"]["Notification"][2]["matcher"], "idle_prompt");
}

#[test]
fn install_keeps_the_owners_hooks_settings_and_key_order() {
    let mut doc = user_settings();
    install(&mut doc, EXE).unwrap();
    let keys: Vec<&String> = doc.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["model", "hooks", "theme"]);
    assert_eq!(
        doc["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "audit.exe"
    );
    assert_eq!(doc["hooks"]["Stop"][0]["hooks"][0]["command"], "notify.exe");
    assert_eq!(doc["hooks"]["Stop"][1]["hooks"][0]["command"], EXE);
}

#[test]
fn installing_twice_changes_nothing() {
    let mut once = user_settings();
    install(&mut once, EXE).unwrap();
    let mut twice = once.clone();
    install(&mut twice, EXE).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn reinstalling_after_dex_moves_replaces_the_old_path() {
    let mut doc = json!({});
    install(&mut doc, r"D:\old\dex.exe").unwrap();
    assert_eq!(status(&doc, EXE).current, 0);
    install(&mut doc, EXE).unwrap();
    let found = status(&doc, EXE);
    assert_eq!((found.dex, found.current), (HOOKS.len(), HOOKS.len()));
}

#[test]
fn uninstall_removes_only_dex_hooks_and_restores_the_original() {
    let original = user_settings();
    let mut doc = original.clone();
    install(&mut doc, EXE).unwrap();
    assert_eq!(remove(&mut doc).unwrap(), HOOKS.len());
    assert_eq!(doc, original);

    let mut fresh = json!({ "model": "opus" });
    install(&mut fresh, EXE).unwrap();
    remove(&mut fresh).unwrap();
    assert_eq!(
        fresh,
        json!({ "model": "opus" }),
        "an emptied hooks object goes away"
    );
}

#[test]
fn only_dex_event_commands_count_as_dex_hooks() {
    assert!(is_dex_hook(
        &json!({ "command": "C:/x/dex.exe", "args": ["event", "stop"] })
    ));
    assert!(is_dex_hook(
        &json!({ "command": "DEX.EXE", "args": ["event"] })
    ));
    assert!(!is_dex_hook(
        &json!({ "command": "C:/x/dex.exe", "args": ["doctor"] })
    ));
    assert!(!is_dex_hook(
        &json!({ "command": "index.exe", "args": ["event"] })
    ));
    assert!(!is_dex_hook(&json!({ "command": "dex event stop" })));
}

#[test]
fn shapes_dex_cannot_edit_are_refused_not_clobbered() {
    assert_eq!(
        install(&mut json!([]), EXE),
        Err(Malformed("the file is not a JSON object"))
    );
    assert_eq!(
        install(&mut json!({ "hooks": [] }), EXE),
        Err(Malformed("\"hooks\" is not an object"))
    );
}

#[test]
fn a_dex_hook_is_recognised_whichever_platform_wrote_its_path() {
    // A settings file can carry either kind of path, and Dex must know its own
    // hooks on every platform - a Unix path does not split on `\`.
    for program in [
        r"C:\Program Files\Dex\dex.exe",
        "/Applications/Dex.app/Contents/MacOS/dex",
        "dex",
    ] {
        let hook = json!({ "type": "command", "command": program, "args": ["event"] });
        assert!(is_dex_hook(&hook), "{program}");
    }
    let other = json!({ "type": "command", "command": r"C:\tools\index.exe", "args": ["event"] });
    assert!(!is_dex_hook(&other));
}
