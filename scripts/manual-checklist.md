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

Kill an idle agent's `claude` process from Task Manager (no hook fires). Within about
twenty seconds the agent is dead: gone from the sidebar, walked out of the office, and the
pane still there as a shell, and the activity log says "... is dead (Claude Code is no longer
running in its pane)". Do the same to an agent the lead spawned: its pane closes too. Run `claude` in that pane again: a new agent appears. While an
agent is starting, or while Claude Code updates itself, nobody is ended.

## Office — by eye, with real agents

The rules are unit-tested and the pane has been run live against fake agents; what needs a person is the part that needs real ones.

1. In a workspace with `claude` running in a labelled pane, choose **Office** in the title bar. It fills the workspace area. One person at a desk: a name, and your pane's label as the role.
2. Ask that agent to spawn two others. In **Office** view each walks from HR to a pod marked "reserved for <name>", one at a time, and sits. The first agent's role becomes **lead**.
3. Make one of them ask permission: on the map it raises a hand with a **?**.
4. Click it. The panel shows who it reports to, its brief, its live screen and recent activity. **Go to pane** focuses its terminal. **Send a memo** to an idle agent wakes it.
5. **Hire an agent** from HR with a task: the agent walks in; back in **Terminal** view its new pane is there, running. At `max_concurrent`, the button reads "Office full".
6. End an agent (`/exit`): it walks back to HR and fades; its pod is vacant and the others have not moved.
7. Resize the window narrow, then short, and open and shut the sidebar: the map scales. Switch Terminal → Office → Terminal: the terminals are exactly as they were, and typing goes to the focused pane.
8. Turn on Windows' *Animation effects: off* (Settings → Accessibility → Visual effects): nobody walks, hands do not tap.

## Office follow-ups - by eye, with real agents

1. **Clock out** an idle spawned agent from its panel: a confirmation, then it walks out and its pane is gone in Terminal view. Clock out one that is mid-turn: it is interrupted first and still leaves. The activity shows "is dead" - its own SessionEnd ran.
2. Tell the lead to dismiss its agents: each ends and its pane closes. Tell a *child* to stop its siblings: Dex refuses, and nobody ends.
3. **Prompt** an idle agent from its panel: the text appears in its terminal as your turn. For an agent at a permission dialog, and for a hire in its first seconds, Prompt is disabled and says why.
4. **Announce** (middle right of the floor): the dialog names who will hear it and who will not; everyone named gets the prompt, nobody else does.
5. Have the lead message three agents: on the map he goes desk to desk - never home in between - talks at each (bubbles taking turns), and goes home at the end. His own chair is empty meanwhile. A memo from you walks nobody.
6. Open an agent's panel: **What they have done** lists its notes, stores, memos and - for a lead - who it hired, newest first, with no idle/working flips.
7. With ten agents the map grows to four rows and scrolls; with five it is two rows and fits.

## The way out - by eye

1. Office view: clock someone out, or let one finish - they walk along the corridor to the EXIT in the left wall, turn, wave, and go. Nobody leaves through HR. With agents in a third row, arrivals and leavers use the aisle beside the break room, not through it.
2. A `config.toml` that still says `view = "cards"` (from 0.2.0): Dex opens in Office, and `dex config show` warns that the cards view was removed.

## Idle antics - by eye, with real agents

1. In **Office** view, let an agent finish its task. A few seconds later it gets up to something: coffee in the break room, a go kart along the corridor, skipping rope, a body roll or a tumble outside its cubicle, a nap on a camp bed, or a song at its desk. It walks there and walks back; while it is out its chair is empty.
2. With several idle at once: coffee drinkers stand in different places in the break room, never on each other; karts set off left or right, turn where they like, and some go round the block below the corridor, either way round. Watch a few rounds: it never does the same thing twice running, and two idle agents do different things.
3. Prompt it while it is out (its panel, or *Announce*): it stops, walks back to its desk, and only then types.
4. Message it from another agent while it is out: it heads back to its desk to be found there. An idle agent that has a message to carry walks home first, then sets off from its desk.
5. With Windows' *Animation effects: off*, idle agents stay in their chairs.
6. Minimize the **Activity** box (bottom left): it becomes a button in the same corner. Have an agent write a note: the button counts it. Restart Dex: it is still minimized. Open it again.
7. Ask the lead to spawn three agents. At its desk it cups its hands and shouts *I need 1 engineer on the floor ASAP!*; as it sends for the second and third the number grows and it is shouted afresh. Each hire comes out of HR a moment after the shout, and one already walking is not sent back by the next shout. Hiring from HR yourself: nobody shouts.
8. Ask an agent something it should check with you first ("shorten the essay, but ask me before overwriting"). When it stops with its question: its pane header, the sidebar dot and the title bar say **needs you**, a toast says what it asked, and in the office it sits with a hand up and a **?** - it does not wander off. Its panel says what it asked; **Prompt** is enabled. Answer: it works, finishes, and only then goes for its coffee.
9. Tell an agent: "ask me a question about anything, but use a period instead of a question mark." It still shows **needs you**, hand up, with the question in a speech bubble at its mouth (a long one wraps and is cut at four lines; hover, or its panel, has all of it). Answer it; when it finishes, the line under its desk shows its closing words in plain italics, and it wanders off.
10. After `dex skill install`, give an agent a decision that is yours without saying how to ask ("create alpha.txt or beta.txt - the name is my decision, find out"). It asks with Claude Code's dialog: **needs you** at once, the real question in its speech bubble and panel (never "permission to AskUserQuestion", and not replaced a few seconds later by "Claude needs your permission"); the panel sends you to its pane, and Prompt is off. Answer in the pane: it carries on.
11. With one agent mid-question, open **Announce**: it names that agent and warns the announcement may be taken for the answer. `dex agent list` shows `idle (asked you: ...)` for an asker and plain `idle` for one that merely finished.
