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

/** The chosen terminal and the choices, and a way to choose; null until asked. */
export function useTerminal(): { view: TerminalView | null; choose: (runtime: string) => Promise<void> } {
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
  }, []);
  const choose = useCallback(async (runtime: string) => {
    setView(await request<TerminalView>("pane.terminal", { terminal: runtime }));
  }, []);
  return { view, choose };
}
