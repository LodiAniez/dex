//! `dex wsl setup | status <distro>`: getting a WSL distro ready for agents.
//!
//! An agent in WSL runs the distro's own Claude Code, which reads the Linux
//! `~/.claude`, not the Windows one. So what `dex hooks install`, `dex mcp
//! install` and `dex skill install` do for Windows has to be done again in
//! there - by the same code, on the distro's files through `\\wsl.localhost` -
//! with one more piece: a `dex` command in `~/.local/bin` that runs the
//! Windows `dex.exe`, because agents run `dex` by name and Linux only has
//! `dex.exe` (Dex runs no daemon inside the distro).

use std::fs;
use std::path::PathBuf;

use clap::Subcommand;
use dex_protocol::{ErrorBody, ErrorCode};
use serde_json::json;

use crate::commands::mcp::{self, At};
use crate::commands::{hooks, skill};
use crate::output::{self, Format};
use crate::wsl;

#[derive(Debug, Subcommand)]
pub enum WslCommand {
    /// Install the `dex` command, Dex's hooks, its MCP server and its skill
    /// into a distro, so agents there report to Dex.
    Setup {
        /// The distro (`dex pane runtimes` lists them), e.g. Ubuntu.
        distro: String,
    },
    /// Show what of that is in place.
    Status {
        /// The distro, e.g. Ubuntu.
        distro: String,
    },
}

/// Where Dex's pieces go in one distro, as Linux and as Windows see them.
struct Place {
    distro: String,
    home: String,
    /// `dex.exe`, as a Linux path.
    dex: String,
    /// `dex-mcp.exe`, as a Linux path.
    mcp: String,
}

impl Place {
    /// A path under the owner's Linux home, as Windows reaches it.
    fn at(&self, linux: &str) -> PathBuf {
        wsl::unc(&self.distro, &format!("{}/{linux}", self.home))
    }

    /// The `dex` command, as Linux sees it.
    fn command(&self) -> String {
        format!("{}/.local/bin/dex", self.home)
    }
}

fn place(distro: &str) -> Result<Place, ErrorBody> {
    let installed = wsl::distros();
    if !installed.iter().any(|name| name == distro) {
        return Err(ErrorBody {
            code: ErrorCode::InvalidArgs,
            message: format!("no WSL distro named {distro:?} is installed"),
            repair: if installed.is_empty() {
                "Install one first (`wsl --install Ubuntu`).".into()
            } else {
                format!("Use one of: {}.", installed.join(", "))
            },
        });
    }
    let failed = |message: String| ErrorBody {
        code: ErrorCode::Internal,
        message,
        repair: format!("Check that {distro} starts (`wsl -d {distro}`), then run this again."),
    };
    let exe = std::env::current_exe()
        .map_err(|err| failed(format!("cannot find this dex.exe: {err}")))?;
    let server = exe.with_file_name("dex-mcp.exe");
    Ok(Place {
        home: wsl::home(distro).map_err(failed)?,
        dex: wsl::linux_path(distro, &exe).map_err(failed)?,
        mcp: wsl::linux_path(distro, &server).map_err(failed)?,
        distro: distro.to_owned(),
    })
}

/// What of Dex is in place in a distro.
struct Report {
    /// `claude --version` there, if Claude Code is installed.
    claude: Option<String>,
    command: bool,
    hooks: bool,
    mcp: bool,
    skill: bool,
}

impl Report {
    fn missing(&self) -> Vec<&'static str> {
        [
            (self.command, "dex command"),
            (self.hooks, "hooks"),
            (self.mcp, "MCP server"),
            (self.skill, "skill"),
        ]
        .into_iter()
        .filter_map(|(there, name)| (!there).then_some(name))
        .collect()
    }
}

fn claude_version(distro: &str) -> Option<String> {
    let out = wsl::run_login(distro, "claude", &["--version"]).ok()?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !version.is_empty()).then_some(version)
}

fn report(place: &Place) -> Report {
    let claude = claude_version(&place.distro);
    let command = fs::read_to_string(place.at(".local/bin/dex"))
        .is_ok_and(|text| text == wsl::shim(&place.dex));
    let hooks = hooks::status_at(&place.at(".claude/settings.json"), &place.dex)
        .is_ok_and(|found| found.all_current());
    // Asking Claude Code needs Claude Code.
    let mcp = claude.is_some()
        && mcp::registration(At::Wsl(&place.distro)).is_ok_and(|found| {
            found.is_some_and(|found| found.user_scope && found.command == place.mcp)
        });
    let skill = skill::source()
        .and_then(|source| skill::state(&source, &place.at(".claude/skills/dex-agentic")))
        .is_ok_and(|state| state == skill::State::Current);
    Report {
        claude,
        command,
        hooks,
        mcp,
        skill,
    }
}

/// Installs everything that can be. Claude Code itself is the owner's to
/// install and sign in to, so without it the MCP server is left for later.
fn setup(place: &Place) -> Result<Vec<String>, ErrorBody> {
    let mut done = Vec::new();
    let file_error = |message: String| ErrorBody {
        code: ErrorCode::Internal,
        message,
        repair: format!("Check that {} starts, then run this again.", place.distro),
    };

    let bin = place.at(".local/bin");
    fs::create_dir_all(&bin)
        .map_err(|err| file_error(format!("cannot create {}: {err}", bin.display())))?;
    let command = place.at(".local/bin/dex");
    fs::write(&command, wsl::shim(&place.dex))
        .map_err(|err| file_error(format!("cannot write {}: {err}", command.display())))?;
    wsl::make_executable(&place.distro, &place.command()).map_err(file_error)?;
    done.push(format!("`dex` in {} runs {}", place.command(), place.dex));

    hooks::install_at(&place.at(".claude/settings.json"), &place.dex)?;
    done.push(format!(
        "Dex's hooks are in {}/.claude/settings.json",
        place.home
    ));

    skill::install(&skill::source()?, &place.at(".claude/skills/dex-agentic"))?;
    done.push("the dex-agentic skill is installed".into());

    if claude_version(&place.distro).is_none() {
        return Err(ErrorBody {
            code: ErrorCode::InvalidArgs,
            message: format!(
                "Claude Code is not installed in {}; everything but the MCP server is set up ({})",
                place.distro,
                done.join("; ")
            ),
            repair: format!(
                "Open a pane in {0}, install Claude Code there \
                 (`curl -fsSL https://claude.ai/install.sh | bash`) and run `claude` once to sign in. \
                 Then run `dex wsl setup {0}` again.",
                place.distro
            ),
        });
    }
    mcp::install_in(At::Wsl(&place.distro), &place.mcp)?;
    done.push("the Dex MCP server is registered".into());
    Ok(done)
}

pub fn run(command: WslCommand, format: Format) -> Result<(), ErrorBody> {
    match command {
        WslCommand::Setup { distro } => {
            let place = place(&distro)?;
            let done = setup(&place)?;
            if format.json {
                output::json(&json!({ "distro": distro, "done": done }));
            } else {
                println!("Set up {distro} for Dex:");
                for line in done {
                    println!("  {line}");
                }
                println!("Agents that start in {distro} from now on report to Dex.");
            }
        }
        WslCommand::Status { distro } => {
            let place = place(&distro)?;
            let found = report(&place);
            if format.json {
                output::json(&json!({
                    "distro": distro,
                    "claude": found.claude,
                    "command": found.command,
                    "hooks": found.hooks,
                    "mcp": found.mcp,
                    "skill": found.skill,
                }));
            } else {
                println!("{}", describe(&distro, &found));
            }
        }
    }
    Ok(())
}

fn describe(distro: &str, found: &Report) -> String {
    let missing = found.missing();
    match (&found.claude, missing.is_empty()) {
        (None, _) => format!(
            "Claude Code is not installed in {distro}; install it there to run agents in it"
        ),
        (Some(_), true) => format!("set up: agents in {distro} report to Dex"),
        (Some(_), false) => format!(
            "not set up for Dex: no {} (run `dex wsl setup {distro}`)",
            missing.join(", no ")
        ),
    }
}

/// One doctor line: the check's name, whether it passed (`None`: not
/// examined, which is not a failure), and what it found.
pub type DistroCheck = (String, Option<bool>, String);

/// For `dex doctor`: one line per distro. A distro no Dex pane runs in is not
/// examined - that would start its VM on every check - and is no failure:
/// nobody has asked for agents there yet.
pub fn doctor_checks(used: &[String]) -> Vec<DistroCheck> {
    wsl::distros()
        .into_iter()
        .map(|distro| {
            let name = format!("wsl:{distro}");
            if !used.contains(&distro) {
                let detail = format!(
                    "no Dex pane runs in {distro} yet; `dex wsl setup {distro}` gets it ready for agents"
                );
                return (name, None, detail);
            }
            match place(&distro) {
                Err(err) => (name, Some(false), err.message),
                Ok(place) => {
                    let found = report(&place);
                    let ok = found.claude.is_some() && found.missing().is_empty();
                    (name, Some(ok), describe(&distro, &found))
                }
            }
        })
        .collect()
}
