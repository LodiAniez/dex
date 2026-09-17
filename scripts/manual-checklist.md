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

## Office — by eye, with real agents

The rules are unit-tested and the pane has been run live against fake agents; what needs a person is the part that needs real ones.

1. In a workspace with `claude` running in a labelled pane, choose **Cards** in the title bar. It fills the workspace area. One card: a name, your pane's label as the role, the last two lines of its terminal.
2. Ask that agent to spawn two others. In **Office** view each walks from HR to a pod marked "reserved for <name>", one at a time, and sits. The first agent's role becomes **lead**.
3. Make one of them ask permission: its card border turns blue, and on the map it raises a hand with a **?**.
4. Click it. The panel shows who it reports to, its brief, its live screen and recent activity. **Go to pane** focuses its terminal. **Send a memo** to an idle agent wakes it.
5. **Hire an agent** from HR with a task: the agent walks in; back in **Terminal** view its new pane is there, running. At `max_concurrent`, the button reads "Office full".
6. End an agent (`/exit`): it walks back to HR and fades; its pod is vacant and the others have not moved.
7. Resize the window narrow, then short, and open and shut the sidebar: cards reflow to two columns then one, the map scales. Switch Terminal → Office → Terminal: the terminals are exactly as they were, and typing goes to the focused pane.
8. Turn on Windows' *Animation effects: off* (Settings → Accessibility → Visual effects): nobody walks, hands do not tap.

## Office follow-ups - by eye, with real agents

1. **Clock out** an idle spawned agent from its panel: a confirmation, then it walks out and its pane is gone in Terminal view. Clock out one that is mid-turn: it is interrupted first and still leaves. The activity shows "is dead" - its own SessionEnd ran.
2. Tell the lead to dismiss its agents: each ends and its pane closes. Tell a *child* to stop its siblings: Dex refuses, and nobody ends.
3. **Prompt** an idle agent from its panel: the text appears in its terminal as your turn. For an agent at a permission dialog, and for a hire in its first seconds, Prompt is disabled and says why.
4. **Announce** from Cards (beside the workspace name) and from Office (middle right): the dialog names who will hear it and who will not; everyone named gets the prompt, nobody else does.
5. Have the lead message three agents: on the map he goes desk to desk - never home in between - talks at each (bubbles taking turns), and goes home at the end. His own chair is empty meanwhile. A memo from you walks nobody.
6. Open an agent's panel: **What they have done** lists its notes, stores, memos and - for a lead - who it hired, newest first, with no idle/working flips.
7. With ten agents the map grows to four rows and scrolls; with five it is two rows and fits.
