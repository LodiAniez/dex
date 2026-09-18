use super::*;

/// A shipped skill and an empty "home", both in one temp dir.
fn fixture(shipped: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("skills").join(NAME).join(FILE);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, shipped).unwrap();
    let target = dir.path().join(".claude").join("skills").join(NAME);
    (dir, source, target)
}

#[test]
fn the_shipped_copy_is_looked_for_beside_the_exe_then_in_a_build_tree() {
    let candidates = source_candidates(Path::new(r"C:\Program Files\Dex"));
    assert_eq!(
        candidates[0],
        PathBuf::from(r"C:\Program Files\Dex")
            .join("skills")
            .join(NAME)
            .join(FILE)
    );
    // target\release\dex.exe → three up is the repository, which has skills\.
    assert!(
        candidates[2].ends_with(
            Path::new("..")
                .join("..")
                .join("skills")
                .join(NAME)
                .join(FILE)
        )
    );
}

#[test]
fn in_a_mac_app_bundle_the_skill_is_in_its_resources() {
    // `dex` is in Dex.app/Contents/MacOS; a bundle keeps data in Contents/Resources.
    let dir = Path::new("/Applications/Dex.app/Contents/MacOS");
    let tail: PathBuf = ["skills", NAME, FILE].iter().collect();
    assert!(
        source_candidates(dir).contains(&dir.join("..").join("Resources").join(tail)),
        "{:?}",
        source_candidates(dir)
    );
}

#[test]
fn install_creates_the_folder_and_copies_the_file() {
    let (_dir, source, target) = fixture("# skill v1\n");
    assert_eq!(state(&source, &target).unwrap(), State::Missing);

    install(&source, &target).unwrap();
    assert_eq!(
        fs::read_to_string(target.join(FILE)).unwrap(),
        "# skill v1\n"
    );
    assert_eq!(state(&source, &target).unwrap(), State::Current);
}

#[test]
fn a_newer_build_shows_the_installed_copy_as_outdated_and_install_refreshes_it() {
    let (_dir, source, target) = fixture("# skill v1\n");
    install(&source, &target).unwrap();

    fs::write(&source, "# skill v2\n").unwrap();
    assert_eq!(state(&source, &target).unwrap(), State::Outdated);

    install(&source, &target).unwrap();
    assert_eq!(state(&source, &target).unwrap(), State::Current);
}

#[test]
fn uninstall_removes_the_file_and_an_emptied_folder_only() {
    let (_dir, source, target) = fixture("# skill\n");
    install(&source, &target).unwrap();

    assert!(uninstall(&target).unwrap());
    assert!(!target.join(FILE).exists());
    assert!(!target.exists(), "an emptied folder goes too");
    assert!(
        target.parent().unwrap().exists(),
        "but the skills folder above it stays"
    );

    // Nothing there: not an error, just nothing to do.
    assert!(!uninstall(&target).unwrap());
}

#[test]
fn uninstall_leaves_a_folder_the_owner_has_put_other_things_in() {
    let (_dir, source, target) = fixture("# skill\n");
    install(&source, &target).unwrap();
    fs::write(target.join("my-notes.md"), "mine\n").unwrap();

    assert!(uninstall(&target).unwrap());
    assert!(!target.join(FILE).exists());
    assert!(
        target.join("my-notes.md").exists(),
        "only the skill file is Dex's to remove"
    );
    assert!(target.exists());
}

#[test]
fn uninstall_refuses_a_folder_that_is_not_the_skills() {
    // The one guard that matters: a wrong path must remove nothing at all.
    let dir = tempfile::tempdir().unwrap();
    let wrong = dir.path().join("something-else");
    fs::create_dir_all(&wrong).unwrap();
    fs::write(wrong.join(FILE), "not ours\n").unwrap();

    let err = uninstall(&wrong).unwrap_err();
    assert!(err.message.contains("refusing"), "{}", err.message);
    assert!(wrong.join(FILE).exists());
}

#[test]
fn doctor_text_says_what_to_run() {
    let (_dir, source, target) = fixture("# skill\n");
    assert!(describe(State::Missing, &target).contains("dex skill install"));
    assert!(describe(State::Outdated, &target).contains("dex skill install"));
    install(&source, &target).unwrap();
    assert!(describe(State::Current, &target).contains("current"));
}
