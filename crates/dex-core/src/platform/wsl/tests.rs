use super::*;

#[test]
fn a_runtime_is_windows_or_a_named_distro() {
    assert_eq!(Runtime::parse("windows"), Ok(Runtime::Windows));
    assert_eq!(
        Runtime::parse("wsl:Ubuntu"),
        Ok(Runtime::Wsl("Ubuntu".into()))
    );
    assert_eq!(
        Runtime::parse("wsl:Ubuntu-24.04"),
        Ok(Runtime::Wsl("Ubuntu-24.04".into()))
    );
    assert_eq!(Runtime::Wsl("Ubuntu".into()).to_string(), "wsl:Ubuntu");
    assert_eq!(Runtime::Windows.to_string(), "windows");
}

#[test]
fn a_distro_name_that_is_not_a_name_is_refused() {
    // Names go to wsl.exe as arguments, never through a shell, but a name with
    // spaces or quotes is not a distro and says the caller got something wrong.
    for bad in [
        "wsl:",
        "wsl:Ubu ntu",
        "wsl:\"x\"",
        "wsl:a;b",
        "linux",
        "wsl:-d",
    ] {
        assert!(Runtime::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_distro_list_wsl_prints_is_read_from_utf16() {
    // `wsl.exe -l -q` prints UTF-16LE, one name per line, CRLF, sometimes
    // with a trailing NUL; Docker Desktop's own distros are not for agents.
    let printed: Vec<u8> = "Ubuntu\r\ndocker-desktop\r\nDebian\r\n\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(parse_distros(&printed), vec!["Ubuntu", "Debian"]);
    assert!(parse_distros(&[]).is_empty());
}

#[test]
fn the_pane_variables_are_added_to_wslenv_without_losing_the_owners() {
    let names = ["DEX_PANE_ID", "DEX_SOCKET"];
    assert_eq!(wslenv(None, &names), "DEX_PANE_ID:DEX_SOCKET");
    assert_eq!(wslenv(Some(""), &names), "DEX_PANE_ID:DEX_SOCKET");
    assert_eq!(
        wslenv(Some("USERPROFILE/p:DEX_SOCKET"), &names),
        "USERPROFILE/p:DEX_SOCKET:DEX_PANE_ID"
    );
    // One the owner already passes with flags is left as they set it.
    assert_eq!(
        wslenv(Some("DEX_PANE_ID/u"), &names),
        "DEX_PANE_ID/u:DEX_SOCKET"
    );
}

#[test]
fn a_pane_starts_its_distro_in_its_folder() {
    assert_eq!(
        pane_args("Ubuntu", "C:/src/api"),
        vec!["-d", "Ubuntu", "--cd", "C:/src/api"]
    );
}

#[cfg(windows)]
#[test]
#[ignore = "needs WSL with a distro installed"]
fn a_real_distro_translates_a_windows_path() {
    let distro = distros().into_iter().next().expect("a distro");
    assert_eq!(linux_path(&distro, "C:/Windows").unwrap(), "/mnt/c/Windows");
}
