//! What `dex wsl setup` checks before it would break something, or to say
//! exactly why `dex` does not run in a distro.

use dex_protocol::{ErrorBody, ErrorCode};

use super::Place;
use crate::wsl;

/// Why `dex` does not run in the distro, as precisely as can be told: the
/// terminal's PATH could not be read; Windows programs cannot run there
/// (interop is off); or `~/.local/bin` is not on the PATH.
pub(super) fn dex_not_found(place: &Place) -> ErrorBody {
    let interop = wsl::run_login(&place.distro, &place.dex, &["--version"])
        .is_ok_and(|out| out.status.success());
    let (message, repair) = match wsl::login_path(&place.distro) {
        Err(why) => (
            format!("`dex` is installed at {}, but {why}", place.command()),
            "Open a terminal in the distro and check that it starts cleanly, then run this again."
                .to_owned(),
        ),
        Ok(_) if !interop => (
            format!("Windows programs cannot run in {}, so neither can `dex`", place.distro),
            "Turn interop back on: remove `enabled=false` under [interop] in /etc/wsl.conf, run `wsl --shutdown`, then run this again."
                .to_owned(),
        ),
        Ok(_) => (
            format!(
                "`dex` is installed at {} but a terminal in {} does not find it",
                place.command(),
                place.distro
            ),
            "Add ~/.local/bin to PATH in your shell's startup file (~/.profile for bash, ~/.zprofile for zsh), then run this again."
                .to_owned(),
        ),
    };
    ErrorBody {
        code: ErrorCode::InvalidArgs,
        message,
        repair,
    }
}

/// Whether `~/.claude`, followed through any links to `real`, is the Windows
/// one - `windows`, as the distro sees it, when that could be found; any
/// folder on a Windows drive otherwise. Pure.
pub(super) fn links_to_windows(real: &str, windows: Option<&str>) -> bool {
    // Windows drives ignore case: `/mnt/c/users/me` is `/mnt/c/Users/Me`.
    let fold = |path: &str| {
        if path.starts_with("/mnt/") {
            path.to_ascii_lowercase()
        } else {
            path.to_owned()
        }
    };
    match windows {
        Some(windows) => {
            let (real, windows) = (fold(real), fold(windows));
            real == windows || real.starts_with(&format!("{windows}/"))
        }
        None => real.starts_with("/mnt/"),
    }
}

/// `~/.claude` as a link to the Windows one (a way to share settings): Dex's
/// Windows hooks live in that file, and rewriting them for Linux would break
/// every Windows agent's. Refused when it could not be checked, too.
pub(super) fn check_not_shared(place: &Place) -> Result<(), ErrorBody> {
    let refuse = |message: String| ErrorBody {
        code: ErrorCode::InvalidArgs,
        message,
        repair: format!(
            "Give {} its own ~/.claude (replace any link with a folder), then run this again.",
            place.distro
        ),
    };
    let real =
        wsl::real_path(&place.distro, &format!("{}/.claude", place.home)).map_err(|err| {
            refuse(format!(
                "could not check where ~/.claude in {} is: {err}",
                place.distro
            ))
        })?;
    let windows = dex_cli::paths::home_dir()
        .and_then(|home| wsl::linux_path(&place.distro, &home.join(".claude")).ok());
    if links_to_windows(&real, windows.as_deref()) {
        return Err(refuse(format!(
            "{}/.claude in {} is {real}, the Windows one; its hooks are Windows Dex's",
            place.home, place.distro
        )));
    }
    Ok(())
}
