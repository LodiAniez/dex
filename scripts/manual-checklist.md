# Manual checklist

What automation cannot judge (docs/prd.md §15). Run the relevant section
before signing off a milestone; note the machine and date in ARCHITECTURE.md.

## M8 — clean machine, installer to four agents

On a clean Windows 11 VM with Claude Code installed and signed in, and Git for
Windows on PATH:

1. Run `Dex_0.1.0_x64_en-US.msi` from `target/release/bundle/msi/`. Accept the
   defaults. Expect: Dex in the Start Menu; `dex --version` works in a **new**
   terminal (PATH changes reach new terminals only).
2. Start Dex. Expect: the setup panel opens by itself, with `app`, `version`,
   `claude` and `git` green and `hooks` and `mcp` red, each with a button.
3. Press **Install hooks**, then **Register MCP server**. Expect: both turn
   green without a restart; the output box shows what each did. Press **Done**.
4. In the first pane, `dex repo add <path to a git repository>`, then run
   `claude`. Expect: the pane header shows a status dot within a few seconds
   of the first prompt; `dex agent list` in another terminal shows it.
5. In Claude Code, ask it to start another agent on a small task with a
   worktree. Expect: a new pane appears, a worktree exists under
   `%USERPROFILE%\dex\worktrees`, and the child gets past the folder-trust
   dialog and begins work with no keypress from you.
6. Repeat step 5 twice more from different panes. Expect: four agents, four
   status dots, the title bar counting them, and the activity pane
   (`activity` button, top right) showing all of them.
7. Switch to another window and wait for one agent to finish or ask a
   question. Expect: a Windows toast titled with the workspace and pane;
   clicking it brings Dex forward on that pane. (This is the AppUserModelID
   check: a dev build shows the toast under PowerShell's name instead.)
8. Uninstall from Settings. Expect: `dex` gone from PATH in a new terminal;
   `%APPDATA%\Dex` and `~/.claude/settings.json` hooks left alone — uninstall
   removes the program, not your data or your Claude Code configuration.

## M1 — throughput by eye

`type` a 50 MB file in a pane. Expect: the terminal stays responsive to
keystrokes throughout, and no garbage in the output when it ends.

## M2 — divider drag

Drag a divider between two panes. Expect: both terminals refit to the new
sizes with no stray characters; a TUI (`vim`, `htop` under WSL) redraws
cleanly after the drag stops.

## M5 — status dots with a real agent

Start `claude` in a pane. Expect the dot to move: idle → running on the first
prompt, waiting when it asks for permission, idle when it stops. Ctrl+C the
agent: the dot goes dead at once, not after the watchdog's ninety seconds.
