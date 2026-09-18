//! The update check: is there a newer Dex on GitHub than the one running?
//!
//! One GET to the releases API, compared against this build's version. Nothing
//! is downloaded or installed — the answer is a pill in the title bar, and
//! clicking it opens the release page in the browser, where the owner reads
//! the notes and decides. Off with `[updates] check = false` in config.toml,
//! and never more than the UI's own schedule (startup, then every few hours).

use std::process::Command;
use std::time::Duration;

use dex_core::app::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Where releases are published. The only place `update_open` will go.
const RELEASES: &str = "https://github.com/LodiAniez/dex/releases";
const LATEST_API: &str = "https://api.github.com/repos/LodiAniez/dex/releases/latest";
/// This build.
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A newer release than this build.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Update {
    /// The newer version, without its `v`.
    pub version: String,
    /// The release page, for the click.
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `a.b.c` as numbers, ignoring a leading `v` and anything after a `-`.
fn parse(version: &str) -> Option<[u64; 3]> {
    let core = version.trim().trim_start_matches('v');
    let core = core.split('-').next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let out = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(out)
}

/// Whether `latest` is a newer release than `current`. Unparseable input is
/// never "newer": a bad tag must not raise a pill that will not go away.
pub fn newer(current: &str, latest: &str) -> bool {
    match (parse(current), parse(latest)) {
        (Some(now), Some(then)) => then > now,
        _ => false,
    }
}

fn fetch_latest() -> Result<Release, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent();
    let text = agent
        .get(LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", &format!("dex/{CURRENT}"))
        .call()
        .map_err(|err| err.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|err| err.to_string())?;
    // serde_json is already here; ureq's own JSON feature would add nothing.
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

/// Asks GitHub for the latest release; `Some` only when it is newer than this
/// build. Network failures are `None` too — an update check that cannot reach
/// GitHub is not news the title bar should carry.
#[tauri::command]
pub async fn update_check(state: State<'_, AppState>) -> Result<Option<Update>, String> {
    if !state.config.get().updates.check {
        return Ok(None);
    }
    let latest = tauri::async_runtime::spawn_blocking(fetch_latest)
        .await
        .map_err(|err| err.to_string())?;
    let release = match latest {
        Ok(release) => release,
        Err(err) => {
            tracing::debug!(%err, "update check did not reach GitHub");
            return Ok(None);
        }
    };
    if release.draft || release.prerelease || !newer(CURRENT, &release.tag_name) {
        return Ok(None);
    }
    Ok(Some(Update {
        version: release.tag_name.trim_start_matches('v').to_owned(),
        url: release.html_url,
    }))
}

/// Opens a release page in the default browser. Only Dex's own releases: the
/// URL came from GitHub's API, but the browser is still not somewhere to send
/// arbitrary text.
#[tauri::command]
pub fn update_open(url: String) -> Result<(), String> {
    if !url.starts_with(RELEASES) {
        return Err(format!("{url:?} is not a Dex release page"));
    }
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(&url)
        .spawn()
        // Waited for on its own thread: on macOS an unwaited child stays a zombie.
        .map(|mut child| {
            std::thread::spawn(move || child.wait());
        })
        .map_err(|err| format!("could not open the browser: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_tag_is_newer_and_an_earlier_or_equal_one_is_not() {
        assert!(newer("0.1.0", "v0.1.1"));
        assert!(newer("0.1.1", "v0.2.0"));
        assert!(newer("0.9.9", "v1.0.0"));
        assert!(!newer("0.1.1", "v0.1.1"));
        assert!(!newer("0.1.1", "v0.1.0"));
        assert!(!newer("1.0.0", "v0.9.9"));
    }

    #[test]
    fn comparison_is_numeric_not_textual() {
        // "0.10.0" > "0.9.0"; a string compare would say otherwise.
        assert!(newer("0.9.0", "v0.10.0"));
        assert!(!newer("0.10.0", "v0.9.0"));
    }

    #[test]
    fn a_tag_it_cannot_read_is_never_newer() {
        // Otherwise a stray tag would raise a pill nobody could dismiss.
        assert!(!newer("0.1.1", "nightly"));
        assert!(!newer("0.1.1", "v0.2"));
        assert!(!newer("0.1.1", "v0.2.0.1"));
        assert!(!newer("0.1.1", ""));
    }

    #[test]
    fn a_prerelease_suffix_compares_by_its_core() {
        assert!(newer("0.1.1", "v0.2.0-rc.1"));
        assert!(!newer("0.2.0", "v0.2.0-rc.1"));
    }

    #[test]
    fn only_release_pages_may_be_opened() {
        assert!(update_open("https://example.com/".into()).is_err());
        assert!(update_open("https://github.com/LodiAniez/other/releases/tag/v1".into()).is_err());
    }
}
