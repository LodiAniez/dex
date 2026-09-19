import { useCallback, useEffect, useState } from "react";
import { request } from "./daemon";
import type { RunningPane } from "./generated/RunningPane";
import type { TerminalView } from "./generated/TerminalView";

/**
 * The terminal Dex opens: `windows` (PowerShell) or `wsl:<distro>`. One
 * choice for the whole app (`pane.terminal`): every new pane and every agent
 * spawned opens there, and when Dex starts, every pane does. A pane already
 * running keeps its shell unless the owner has it restarted (`restartChoices`).
 */

/** The distro of a WSL terminal; null for Windows or none. */
export function distroOf(runtime: string | undefined): string | null {
  return runtime?.startsWith("wsl:") ? runtime.slice(4) : null;
}

/** A terminal as the owner reads it: "PowerShell", "Ubuntu (WSL)". */
export function runtimeLabel(runtime: string): string {
  if (runtime === "windows") return "PowerShell";
  const distro = distroOf(runtime);
  return distro ? `${distro} (WSL)` : runtime;
}

/**
 * Whether this window can close a pane's shell and start it again in `next`:
 * it is running here - not exited, not shown in another window - and in
 * another terminal. Whether it should is the owner's call.
 */
export function switchNow(
  shell: { spawned: boolean; dead: boolean; away: boolean; runtime?: string },
  next: string,
): boolean {
  return shell.spawned && !shell.dead && !shell.away && shell.runtime !== undefined && shell.runtime !== next;
}

/** One pane in the "restart these in the new terminal?" list. */
export interface RestartChoice {
  pane: string;
  title: string;
  detail: string;
  busy: boolean;
  /** Ticked to begin with: a plain shell is; one that may be busy is not. */
  ticked: boolean;
}

/**
 * The panes to offer to restart after a terminal is chosen: those running
 * elsewhere that this window can restart (not ones shown in another window).
 * Plain shells start ticked; a pane where something may be running does not,
 * and says so - restarting it would end that. A pane with a live agent is
 * busy whatever the process table says, and names the agent.
 */
export function restartChoices(
  running: readonly RunningPane[],
  canRestart: (pane: string) => boolean,
  agentIn: (pane: string) => string | null = () => null,
): RestartChoice[] {
  return running
    .filter((pane) => canRestart(pane.pane))
    .map((pane) => {
      const agent = agentIn(pane.pane);
      const busy = pane.busy || agent !== null;
      const why = agent !== null ? ` · agent ${agent} runs here` : busy ? " · something may be running in it" : "";
      return {
        pane: pane.pane,
        title: pane.label ?? pane.cwd.split(/[\\/]/).filter(Boolean).at(-1) ?? pane.cwd,
        detail: `${pane.workspace} · ${runtimeLabel(pane.runtime)}${why}`,
        busy,
        ticked: !busy,
      };
    });
}

/**
 * Of the panes the owner ticked, the ones to restart now, asked again at the
 * moment of the click: one no longer running elsewhere is left, and one that
 * was plain when offered but is busy now is skipped - something started in it
 * since. A pane offered as busy and ticked anyway is the owner's decision.
 */
export function stillToRestart(
  ticked: ReadonlySet<string>,
  offered: readonly RestartChoice[],
  now: readonly RestartChoice[],
): { restart: string[]; skipped: string[] } {
  const current = new Map(now.map((choice) => [choice.pane, choice]));
  const restart: string[] = [];
  const skipped: string[] = [];
  for (const choice of offered) {
    if (!ticked.has(choice.pane)) continue;
    const fresh = current.get(choice.pane);
    if (!fresh) continue;
    if (fresh.busy && !choice.busy) skipped.push(choice.title);
    else restart.push(choice.pane);
  }
  return { restart, skipped };
}

/**
 * What the terminal picker offers: every terminal there is, or nothing where
 * Windows is all there is. A chosen distro that has since been uninstalled
 * stays in the list, marked, so the owner can see it and choose away from it.
 */
export function terminalChoices(view: TerminalView): { value: string; label: string }[] {
  const gone = !view.runtimes.includes(view.terminal);
  if (view.runtimes.length < 2 && !gone) return [];
  const choices = view.runtimes.map((value) => ({ value, label: runtimeLabel(value) }));
  if (gone) choices.push({ value: view.terminal, label: `${runtimeLabel(view.terminal)} - not installed` });
  return choices;
}

/**
 * The chosen terminal and the choices, and a way to choose; null until asked.
 * Asked again whenever `generation` changes - the setup panel's checks - so a
 * distro installed meanwhile, or a first ask that failed, is caught up with.
 */
export function useTerminal(generation?: unknown): {
  view: TerminalView | null;
  choose: (runtime: string) => Promise<TerminalView>;
} {
  const [view, setView] = useState<TerminalView | null>(null);
  useEffect(() => {
    let live = true;
    void request<TerminalView>("pane.terminal", {}).then(
      (answer) => live && setView(answer),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [generation]);
  const choose = useCallback(async (runtime: string) => {
    const answer = await request<TerminalView>("pane.terminal", { terminal: runtime });
    setView(answer);
    return answer;
  }, []);
  return { view, choose };
}
