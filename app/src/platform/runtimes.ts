import { useCallback, useEffect, useState } from "react";
import { request } from "./daemon";
import type { TerminalView } from "./generated/TerminalView";

/**
 * The terminal Dex opens: `windows` (PowerShell) or `wsl:<distro>`. One
 * choice for the whole app (`pane.terminal`): every new pane and every agent
 * spawned opens there, and when Dex starts, every pane does. A pane already
 * running keeps its shell.
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
  choose: (runtime: string) => Promise<void>;
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
    setView(await request<TerminalView>("pane.terminal", { terminal: runtime }));
  }, []);
  return { view, choose };
}
