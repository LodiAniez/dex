<img src="app/src-tauri/icons/128x128@2x.png" alt="Dex" width="96" align="right">

# Dex

**One window, several Claude Code agents, and a shared memory between them.**

Dex is a terminal workspace for multi-agent coding, on Windows and macOS. You open a workspace per project, split it into panes, and run a Claude Code agent in each. Dex watches what every agent does, shows you their status at a glance, tells you when one needs you, and gives the agents a place to leave notes for each other — so four agents working on one codebase stop solving the same problem four times.

An agent in Dex can also start another agent: on its own task, in its own pane, in its own git worktree, and it gets to work without anyone typing anything.

---

## What it's for

You have a job that's bigger than one agent should do alone, or several jobs at once against the same repository:

- **Parallel work on one codebase.** Port the API in one pane, write its tests in another, update the docs in a third — each agent in its own worktree, so nobody clobbers anybody, with a diff pane open to watch what changes.
- **A lead and workers.** Ask one agent to plan; it spawns workers for the independent parts, with a written brief each, and reads their notes as they finish.
- **Not losing track.** Status dots on every pane, a title bar counting who's working and who's waiting, a toast when an agent in a pane you're not looking at needs you, and a live activity stream of everything every agent has said and done.
- **Agents that remember what their siblings found out.** A shared context store per workspace: notes, durable facts under keys, directed messages. Each agent gets a short digest of what changed at the start of its turns, automatically.

It is a terminal first. Every pane is a real shell (PowerShell by default on Windows, your login shell on macOS) in a real pseudo-terminal, and you can use it with no agents at all.

## Where it runs

- **Windows 11**, or Windows 10 1809 or later (ConPTY is required). WebView2 is preinstalled on Windows 11.
- **macOS 11** or later, on Apple Silicon or Intel. macOS support is new and less tried than Windows; please [report](https://github.com/LodiAniez/dex/issues) what goes wrong.
- **Claude Code** installed and signed in with **your own Anthropic account** — a Claude Pro or Max subscription, or an API key with billing. Dex drives Claude Code; it does not replace it, and it has no account, keys or usage of its own. Every agent you run in Dex, including the ones agents spawn, is an ordinary Claude Code session on your plan and counts against your usage exactly as if you had opened a terminal and typed `claude`. Any account Claude Code accepts works — Pro, Max, API billing, Bedrock or Vertex — because Dex never touches the credentials.

  **Claude Code only.** Status dots, toasts, shared context and spawning all come through Claude Code's hooks and MCP registration. Other coding agents (Codex, Gemini CLI, Aider…) run fine in a Dex pane as plain terminals, but Dex will not know they exist. Supporting them would take an adapter per agent; none is planned for v1.
- **Git** on PATH — worktrees and the diff pane need it. On Windows that is Git for Windows; on macOS, the Xcode Command Line Tools (`xcode-select --install`) or Git from Homebrew.

Not on Linux yet. Running agents inside WSL from Dex is on the roadmap but not in this release; on Windows, panes run Windows shells.

## Install

### Windows

1. Download the latest `Dex_<version>_x64_en-US.msi` from the **[Releases page](https://github.com/LodiAniez/dex/releases)**.
2. Run it and accept the defaults. It installs Dex to Program Files, adds the `dex` command to your PATH, and puts Dex in the Start Menu.

   Windows will show *"Windows protected your PC"* because this build is not code-signed yet — click **More info**, then **Run anyway**. The UAC prompt says *Unknown publisher* for the same reason. The installer is exactly what's in this repository; signing is a paid identity check, not a change to the file.
3. Open a **new** terminal (PATH changes reach new terminals only) and confirm:

   ```powershell
   dex --version
   ```

### macOS

1. Download the `.dmg` for your Mac from the **[Releases page](https://github.com/LodiAniez/dex/releases)**: `Dex_<version>_macos_apple-silicon.dmg` for an M-series Mac, `Dex_<version>_macos_intel.dmg` for an Intel one. **About This Mac** says which you have.
2. Open it and drag **Dex** to **Applications**.
3. The first time you open Dex, macOS says it cannot check it for malicious software, because this build is not signed or notarized by Apple yet. Click **Done**, then open **System Settings → Privacy & Security**, scroll down to the message that Dex was blocked, and click **Open Anyway**. The button is there for about an hour after the blocked launch. macOS asks once more, and for your password; after that Dex opens normally. From a terminal, this does the same, and also covers the `dex` command inside the app:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Dex.app
   ```

Every Dex pane already has the `dex` command. To use it in other terminals too, put Dex's folder on your PATH, then open a new terminal:

```sh
echo 'export PATH="/Applications/Dex.app/Contents/MacOS:$PATH"' >> ~/.zprofile
```

That is for zsh, the Mac's default shell; for bash, use `~/.bash_profile`. Add the folder rather than a symlink: `dex` finds `dex-mcp` and its skill beside itself. If macOS blocks `dex` in another terminal, run the `xattr` command above.

Neither installer touches your Claude Code configuration. That happens in the next step, in your own session, with your click.

## Set up

Start Dex. The first time, a setup panel opens on its own and runs the same checks as `dex doctor`:

| Check     | What it means                                              | If it's red                                    |
|-----------|------------------------------------------------------------|------------------------------------------------|
| `app`     | Dex is running and answering on its control pipe or socket | Restart Dex                                    |
| `version` | The app and the `dex` command are from the same build      | Reinstall                                      |
| `claude`  | Claude Code is on PATH                                     | Install Claude Code, open a new terminal       |
| `git`     | Git is on PATH                                             | Install Git (see *Where it runs*)              |
| `hooks`   | Dex's hooks are in your Claude Code settings               | Press **Install hooks**                        |
| `mcp`     | Dex's MCP server is registered with Claude Code            | Press **Register MCP server**                  |
| `skill`   | The `dex-agentic` skill is in your Claude Code skills      | Press **Install skill**                        |
| `wsl:<distro>` | A WSL distro is set up for agents (checked once Dex runs panes there, or it is your terminal) | Install Claude Code in the distro, then press **Set up <distro>** |

Press the three buttons. All go green without a restart. That is the whole setup; the panel is in the command palette as **Setup checks** if you ever want it back, and it reappears by itself if something new goes wrong — including after a Dex upgrade, when `skill` turns red until you refresh it.

From a terminal, the same three steps are:

```powershell
dex hooks install     # adds Dex's hooks to %USERPROFILE%\.claude\settings.json; your other hooks are kept
dex mcp install       # runs `claude mcp add` for the Dex server, user scope
dex skill install     # copies the skill to %USERPROFILE%\.claude\skills\dex-agentic\
dex doctor            # the table above
```

`dex hooks install` keeps a one-time backup of your settings at `settings.json.dex-backup` and only ever adds or removes its own entries.

## Using Dex

**Moving panes.** Drag a pane by its header and drop it on another: on an edge to sit beside it there, taking half its space; in the middle to trade places. A highlight shows where it will land; Escape, or letting go anywhere else, puts it back. `Alt+Shift+Arrow` does the same from the keyboard, one step at a time.

**A pane in a window of its own.** The ↗ button in a pane's header opens it in a separate window - put it on another monitor, resize it, leave it behind Dex. The terminal moves with its scrollback and whatever is running in it; the space it left in the layout closes up. **Dock ↩** in that window, or its own close button, puts it back exactly where it was. *Go to pane* (from the office or a toast) brings its window forward. Closing Dex's main window closes them all. The last pane in the main window stays in it.

**Workspaces** live in the left sidebar. One per project, each with a name, a colour, and a root folder. They persist: close Dex and reopen it and your workspaces, panes and layout are back (the terminals restart; scrollback does not survive a restart in this version).

**Panes** are splits within a workspace. A pane is one of:

- a **terminal** (the default) — a shell in the pane's folder;
- an **agent** — just a terminal where you ran `claude`; Dex notices;
- **activity** — the workspace's live event stream. The `activity` button, top right (or *Show activity* in the palette), opens it as a popup over whatever view you are in, and Escape closes it; `dex pane create --kind activity` docks it as a pane instead. Hover an event for **×** to remove it; **clear ended** removes everything agents that have since ended did, **clear all** empties the log. From a terminal: `dex context delete <seq>`, `dex context clear [--all]`;
- **diff** — a repository's uncommitted changes, unstaged or staged, following the working tree as it changes (**Show git diff** in the palette);
- **markdown** — a rendered file that updates as it's written, for agents' notes and plans (`dex pane create --kind markdown --path notes.md`).

**Agents.** Run `claude` in any pane. Within a few seconds its header shows a status dot — running, waiting on you, idle, error, or dead — and the title bar counts them across all workspaces. When an agent in a pane you are not looking at stops or needs input, you get a Windows toast; clicking it brings you to that pane.

### Two views

A workspace can be looked at two ways, switched from **Terminal · Office** in the title bar (or *Terminal view*, *Office view*, *Next view* in the palette — none has a default key, but `"cycle-view" = "Ctrl+Shift+V"` under `[keys]` gives you one). Office takes over the whole workspace area; your panes keep running underneath, and **Terminal** brings them back exactly as they were. Pane shortcuts never act on panes you cannot see: from Office, the first press just brings the panes back.

The office shows every agent in the workspace as a person: a name and a look worked out from the agent's id (so they are the same after a restart), and as their role the label you gave their pane — or, for a spawned agent, the start of its brief. An agent that has spawned others is the **lead**.

The office is a floor plan. Someone typing is working; a raised hand and a **?** needs you - at a permission dialog, or because they ended their turn by asking you something ("Reply yes and I'll overwrite the file"), which Claude Code counts as finished and Dex does not; a red **!** has stopped; a still figure with a dim screen is idle. An agent that spawns others shouts to HR for them - *I need 3 engineers on the floor ASAP!*, the number growing as it sends for more - and each spawned agent then walks in from HR to its pod. One that ends - clocked out, dismissed by its lead, or simply finished - walks along the corridor to the exit in the left wall, turns, waves, and goes; HR is only ever where people are hired. When one agent messages another, the sender gets up and carries it to the recipient's desk, where the two are seen talking it over. A lead briefing three agents makes one round of it — desk to desk, home only at the end — and a message sent while they are already out simply joins the round. An agent whose task is done does not just sit there: it wanders off for a coffee in the break room (wherever there is room to stand), takes a go kart up the corridor or round the block, either way, skips rope, rolls or tumbles about outside its cubicle, naps on a camp bed or sings at its desk - one thing after another, never the same twice running. Give it work and it stops, walks back to its desk like a normal person, and only then is shown working. The **Activity** box, bottom left, says the last few things that happened; minimize it and it becomes a button in the same corner that counts what you have missed, and stays minimized until you open it again. If you have asked Windows for reduced motion, nobody walks — they are simply there, or gone.

Click anyone to see their work. If they need you, the panel says what for at the top — "Waiting for you: permission to Edit …/legacy/client.rs" — and that line takes you to their pane to answer it. For a command it names what runs and leaves out what it carries (`curl -H …`), since arguments are where tokens live, and other agents are told only that a colleague is waiting, not what for. Below it: who they report to, their brief, their screen live, **what they have done** — their own record, newest first: the notes they wrote, what they stored, who they messaged, who they hired, without the idle/working flips — **Go to pane** (back to Terminal view, on their pane), and **Memo** — an ordinary Dex message, so it wakes them if they are idle. **Prompt** types into their terminal as your turn — what you would have typed had you gone to the pane — and is not the same as **Memo**, which goes to their inbox for them to read. Dex only types at an agent while Claude Code is there to read it: not at a permission dialog (the text would answer the dialog), and not at a hire in its first seconds or an agent whose Claude Code has crashed or been quit, whose pane is a bare shell that would *run* the text. It checks the pane's processes before typing; an agent found gone is ended there and then. **Announce** (the loudspeaker, middle-right of the floor) sends one prompt to every agent at once, tells you beforehand who will hear it, and skips anyone waiting on a dialog. In Terminal view you would simply tell the lead. **Clock out** ends them properly: after a confirmation Dex sends `/exit` (interrupting first if they are mid-turn, and falling back to Ctrl+C only if they are still there a few seconds later) and closes their pane; they walk out, and their notes and branch stay. From a terminal: `dex agent stop <label> --graceful --close-pane`. **Hire an agent** in HR spawns one exactly as `dex agent spawn` does, with the same limits; HR counts seats against `agents.max_concurrent` and says so when the office is full.

Names are for reading. Agents still message each other by **label**, which is why the office always shows the label beside the name. The office changes nothing about how agents run; it is another way of looking at what Dex already tracks. Dex remembers the view you last chose; `[ui] view` in the config sets what it opens as before you have chosen.

**Repositories and worktrees.** Register a repo once (`dex repo add <path>`, or `dex repo scan <folder>` to find several). Then any agent — or you — can get a fresh worktree on a new branch under `%USERPROFILE%\dex\worktrees\<repo>\<branch>`, so parallel agents never share a working tree.

**PowerShell or WSL.** On Windows with WSL installed, choose the terminal Dex opens in: **Terminal** at the top of the setup panel (*Setup checks* in the palette), or `dex pane terminal wsl:Ubuntu` (`dex pane terminal windows` to go back; `dex pane terminal` shows the choices). Everything new then opens there - a new workspace, a split, and every agent spawned, by you or by another agent - and so does every pane whose shell has not started yet; when Dex starts, every pane does. A pane whose shell is running keeps it until Dex restarts. Panes in WSL say their distro in the header. If the chosen distro is uninstalled, Dex opens in PowerShell again and says so.

Before the first agent in a distro, get it ready once:

1. Install Claude Code inside the distro and sign in (in a pane there: `curl -fsSL https://claude.ai/install.sh | bash`, then `claude`). Agents in WSL use that Claude Code, with its own settings in Linux.
2. Run `dex wsl setup Ubuntu`, or press **Set up Ubuntu** in the setup panel, which lists every installed distro. It puts a `dex` command in `~/.local/bin` that runs Dex's Windows `dex.exe`, and installs Dex's hooks, MCP server and skill into the distro's Claude Code. Panes already open in the distro find `dex` once restarted. `dex wsl status Ubuntu` checks it all, including that `dex` really runs there. Setup refuses when the distro's `~/.claude` is a link to your Windows one, whose hooks are Windows Dex's.

Nothing of Dex runs inside the distro: agents there talk to the Windows app through `dex.exe`, and Dex asks the distro which of its panes run Claude Code, so an agent there that crashes is ended like any other.

**Worktrees for agents in WSL.** Give an agent in WSL a worktree (`--repo … --worktree …`): Dex makes it with the distro's own git and `--relative-paths`, so git on both sides reads it cleanly - a checkout made by Git for Windows looks modified throughout to Linux git, because of line endings, and its worktrees' links are `C:/…` paths Linux git cannot follow (a pane in one stays on Windows when you switch to WSL). This needs git 2.48 or later on both sides, and marks the repository's format so that an older git, or a tool built on an older libgit2, refuses the repository afterwards; Dex checks both versions first and says how to update.

### Keyboard shortcuts

App shortcuts stay off plain `Ctrl+<letter>`, which your shell owns (`Ctrl+C`, `Ctrl+D`, `Ctrl+W`…), following Windows Terminal.

On macOS every `Ctrl` in this table is `Cmd`, as Mac apps expect, and every `Alt` binding is `Cmd+Option`: Option+Arrow on its own stays with the shell and Claude Code, which use it to jump by word. A binding under `[keys]` is taken as written, and can name `Cmd` on either platform. One that takes another action's key is reported.

| Action                        | Keys                                |
|-------------------------------|-------------------------------------|
| Command palette               | `Ctrl+Shift+P`                      |
| New workspace                 | `Ctrl+Shift+N`                      |
| Switch to workspace 1–9       | `Ctrl+1` … `Ctrl+9`                 |
| Next / previous workspace     | `Ctrl+Shift+PageDown` / `PageUp`    |
| Split pane right / down       | `Ctrl+Shift+D` / `Ctrl+Shift+E`     |
| Close pane                    | `Ctrl+Shift+W`                      |
| Focus pane in a direction     | `Alt+←` `Alt+→` `Alt+↑` `Alt+↓`     |
| Move pane in a direction      | `Alt+Shift+Arrow`, or drag its header onto another pane |
| Toggle zoom on the pane       | `Ctrl+Shift+Enter`                  |
| Cycle layout preset           | `Ctrl+Shift+Space`                  |
| Toggle sidebar                | `Ctrl+Shift+B`                      |

The **command palette** fuzzy-searches workspaces, panes and commands. Type `w:` to search only workspaces, `p:` for only panes. Commands with no default key — *Show git diff*, *Show activity*, the views, *Setup checks* — live there.

Every shortcut can be rebound in the config file (below).

### Configuration

Settings live in `%APPDATA%\Dex\config.toml` and are **hot-reloaded** — save the file and Dex picks it up, no restart. Every setting is optional; the file may not even exist.

```toml
shell = "C:/Program Files/PowerShell/7/pwsh.exe"   # default: pwsh, then powershell, then cmd
worktree_base = "D:/work/worktrees"                # default: %USERPROFILE%\dex\worktrees

[agents]
max_depth = 2                # how deep spawning may go
max_concurrent = 10          # live agents per workspace
spawn_permission_mode = "auto"   # what spawned agents run with
spawn_chrome = false         # true lets spawned agents connect to Claude in Chrome

[digest]
full_chars = 2000            # budget for an agent's opening briefing
delta_chars = 800            # budget for "what changed since your last turn"

[ui]
view = "terminal"            # what a workspace opens as: "terminal" or "office"

[keys]                       # overrides only; anything not listed keeps its default
"command-palette" = "Ctrl+Alt+P"
"split-right" = "Ctrl+Shift+Right"
```

A value Dex can't use is replaced by its default and reported — it never stops Dex starting. `dex config show` prints the settings in force with the defaults filled in; `dex config path` tells you where the file is.

## How agents talk to Dex

Two mechanisms, both installed by the setup panel, both invisible once they are.

### Hooks

Claude Code lets you register commands that run at points in an agent's life. Dex installs ten, all of the form `dex event <kind>`, in your user-level Claude Code settings:

`SessionStart`, `UserPromptSubmit`, `PostToolBatch`, `PermissionRequest`, three `Notification` matchers (permission prompt, needs input, idle), `Stop`, `StopFailure`, `SessionEnd`.

They are how Dex knows an agent's status without reading its screen. Each takes about ten milliseconds. Outside a Dex pane they exit immediately and do nothing, so Claude Code sessions you run elsewhere are unaffected — the hooks check for a Dex pane before doing anything else.

**When an agent needs you.** The certain way is Claude Code's own question dialog (the `AskUserQuestion` tool): it fires a hook, so Dex shows the agent as **needs you** the moment it asks, with the question itself - *Which file should I create?* - in a speech bubble at its mouth. Dex tells every agent, in the context it gets when its session starts, that this is how you hear a question, and the `dex-agentic` skill says the same at more length; in testing an agent given a decision that was yours chose the dialog unprompted. You answer a dialog in the agent's pane.

Agents do not always use it. An agent that ends its turn by asking you something is, to Claude Code, idle: the turn is over. Dex reads how the closing message ends and, if it asks for an answer, shows the agent as **needs you** with what it asked - in the pane header, the sidebar, the title bar's count, a toast, and the office - while keeping it promptable, since answering is just a prompt. It is a careful guess: only the last two sentences (or list items) count, and a phrase counts only as whole words; a sentence asks if it ends in a question mark, uses a phrase like *reply "yes"*, or is simply built as a question ("what would it be.", "Would you like the long version.") whatever it ends with; and "let me know if you need anything else" is not one. Because a guess can miss, the office puts **the question in a speech bubble at the agent's mouth** - they are asking, hand up, and waiting to hear - and shows what any other idle agent said last in a quiet line under their desk, and in their panel, so you can see for yourself.

Hooks cannot tell Dex everything: Claude Code quit with Ctrl+C, crashed or killed says nothing on its way out. So Dex also looks: **an agent whose pane no longer runs Claude Code is dead.** Every fifteen seconds it checks that each started agent still has a Claude Code process in its pane, looks again two seconds later to be sure, and ends the ones that do not - they leave the agent list, walk out of the office, and stop counting toward `agents.max_concurrent`. The workspace log says why, so a lead learns its agent crashed. A pane Dex made for a spawned agent closes with it; a pane you opened stays, and running `claude` in it again starts a new agent. Panes running inside WSL are not checked.

Two of them also carry information *into* the agent: on `SessionStart`, a full digest of the workspace; on `UserPromptSubmit` and `PostToolBatch`, a short delta of what other agents have done since the agent last looked. Nothing is injected when nothing changed.

### The MCP server

`dex-mcp` is a small MCP server registered with Claude Code at user scope. Inside a Dex pane it offers eight tools; outside one it offers none, so it costs nothing in your other projects.

| Tool             | What it does                                                                 |
|------------------|------------------------------------------------------------------------------|
| `note_append`    | Log something the other agents would want to know — a decision, a rename     |
| `context_write`  | Store a durable fact under a key (`auth/jwt`), optionally with a version check |
| `context_read`   | Read one key, with its version                                               |
| `context_list`   | List keys, who wrote them, when — no values                                  |
| `context_search` | Full-text search over everything stored in the workspace                     |
| `agents_list`    | Who else is here and what each is doing                                      |
| `message_send`   | A note to one specific agent, by its pane label                              |
| `message_inbox`  | Read (and clear) messages addressed to you                                   |

### Context sharing

Everything above lands in one store per workspace, kept by Dex in SQLite and mirrored as Markdown under `<workspace root>/.dex/` so you can read it too. Three kinds of thing live there:

- **Notes** — freeform, append-only, timestamped. "Switched the auth module to JWT; tokens expire after 15 minutes."
- **Entries** — durable facts under lowercase, `/`-namespaced keys, with versions. Two agents writing one key with `--expected-version` get a conflict instead of a silent overwrite.
- **Messages** — directed at one agent, read once. A message to an agent that has finished its task and is sitting idle wakes it: Dex types a one-line prompt into its pane telling it to read its inbox. That is how a parent changes a child's task after the fact without stopping it and starting another.

Agents never see their own events echoed back, and status changes are logged for you (the activity stream) but kept out of other agents' digests — a sibling flipping between idle and running twenty times a turn is not news. Digests are written as plain facts, never as instructions, so they cannot be mistaken for prompt injection.

You can read and write the same store from a terminal:

```powershell
dex context note "moved the schema to db/schema.sql"
dex context write api/base-url "http://localhost:8080" --tags api
dex context search "token expiry"
dex context digest        # exactly what an agent would be told right now
```

## Spawning agents

An agent (or you) can start another agent on a task:

```powershell
dex agent spawn --task "port the remaining call sites to the new client API" --repo api --worktree fix/client --label porter
```

Dex creates the worktree, splits a pane for it (beside the lead's earlier hires, not off the lead again - six spawns leave the lead its half and stack the six beside it; and a workspace you have arranged with a *main* or *tiled* preset stays arranged as agents arrive), records the task, starts Claude Code there, and gives it its brief through its opening digest — the task never passes through a shell command line. The child answers Claude Code's folder-trust dialog by itself (only in a worktree Dex created, from a repository you registered) and begins work. Its pane header shows the permission mode it runs in, so an unattended agent is never unattended invisibly.

Guardrails are in Dex, not in prose an agent might skip: **depth 2** (a child may spawn, its child may not) and **ten live agents per workspace**. A refused spawn says why and what to do instead.

Each child is a full Claude Code session under your account, so ten agents spend roughly ten times what one does. The limits exist for your usage as much as for your machine; lower `max_concurrent` in the config if you want a tighter cap.

`dex agent stop <label>` ends an agent. An agent may stop only itself and the agents it spawned — a child told to "stop the others" is refused — while you may stop anyone. If Dex spawned it, its pane is closed with it — Dex made that pane — so a lead told to dismiss its agents does not leave a row of empty shells. An agent you started by typing `claude` keeps its pane: that one is yours.

The judgment side — when spawning helps and when it just costs a cold start, how to write a brief, what to do while waiting — is the `dex-agentic` skill (`skills/dex-agentic/SKILL.md`), which `dex skill install` puts where Claude Code finds it. Agents load it when a task looks splittable.

## The `dex` command

Everything the UI does, the CLI does too, and it is what agents use:

```
dex doctor                              what's running, what's set up
dex workspace  list | create | switch | delete
dex pane       list | create | split | close | send | send-key | label | id
dex agent      list | stop | spawn
dex repo       add | list | scan | status
dex worktree   add | remove | list
dex context    read | write | list | note | send | inbox | search | digest
dex hooks      install | uninstall | status
dex mcp        install | uninstall | status
dex skill      install | uninstall | status
dex config     path | show | reload
```

Add `--json` to any command for machine-readable output. Inside a Dex pane, commands know which pane and workspace they are in; elsewhere, `--workspace <name>` says.

## Updates

Dex checks GitHub for a newer release when it starts and every six hours. When there is one, a **new · update x.y.z** pill appears in the title bar; clicking it opens the release page, where you read the notes and run the new installer — Dex never downloads or installs anything by itself. The check is one request to GitHub's public API and sends nothing but Dex's version; turn it off with

```toml
[updates]
check = false
```

## Uninstall

Settings → Apps → Dex → Uninstall. That removes the program and the PATH entry. It leaves your data (`%APPDATA%\Dex`), your worktrees, and your Claude Code configuration alone; to take the hooks, the MCP registration and the skill out first, run `dex hooks uninstall`, `dex mcp uninstall` and `dex skill uninstall`.

## Building from source

Rust (stable, MSVC) with the Visual Studio C++ build tools, and Node.js 22+.

```powershell
cd app
npm install
npm run tauri dev        # development app
npm run tauri build      # release app and the MSI, in target/release/bundle/msi
```

For how the code is organised, see `docs/conventions.md` and `ARCHITECTURE.md`.

## License

Apache License 2.0 — see [`LICENSE`](LICENSE). Dex is not affiliated with Anthropic; Claude and Claude Code are their trademarks.
