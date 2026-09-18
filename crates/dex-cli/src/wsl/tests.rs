use super::*;

#[test]
fn the_distros_wsl_lists_are_read_from_utf16_less_docker_desktops() {
    let printed: Vec<u8> = "Ubuntu\r\ndocker-desktop-data\r\nDebian\r\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(parse_distros(&printed), vec!["Ubuntu", "Debian"]);
}

#[test]
fn a_linux_path_is_reached_from_windows_through_wsl_localhost() {
    let path = unc("Ubuntu", "/home/me/.claude/settings.json");
    assert_eq!(
        path,
        std::path::PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me\.claude\settings.json")
    );
    // A network path, not a folder named wsl.localhost on the current drive -
    // which is what one lost backslash makes of it, and writes land there.
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let prefix = path.components().next();
        assert!(
            matches!(prefix, Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::UNC(server, share) if server == "wsl.localhost" && share == "Ubuntu")),
            "{path:?}"
        );
    }
}

#[test]
fn the_dex_command_in_linux_runs_the_windows_one_with_its_arguments() {
    assert_eq!(
        shim("/mnt/c/Program Files/Dex/dex.exe"),
        "#!/bin/sh\n# Written by `dex wsl setup`: Dex's CLI is the Windows one.\nexec '/mnt/c/Program Files/Dex/dex.exe' \"$@\"\n"
    );
    // A quote in the path cannot end the quoting early.
    assert!(shim("/mnt/c/it's/dex.exe").contains(r"'/mnt/c/it'\''s/dex.exe'"));
}
