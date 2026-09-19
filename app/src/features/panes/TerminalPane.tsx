import { useEffect, useRef } from "react";
import {
  attachTerminal,
  detachTerminal,
  fitTerminal,
  focusTerminal,
  openTerminal,
} from "../../platform/terminalRegistry";

interface Props {
  paneId: string;
  /** The pane's workspace; its shell gets it as DEX_WORKSPACE_ID. */
  workspaceId: string;
  cwd?: string;
  /** Where its shell runs: `windows` or `wsl:<distro>`. */
  runtime?: string;
  /** The workspace's focused pane: its terminal takes keyboard focus. */
  active: boolean;
}

/**
 * A layout box that shows one pane's terminal. Mounting attaches the terminal;
 * unmounting only detaches it — the terminal and its PTY live on in the
 * registry until the pane is closed (PRD §7.3). Splits, closes, and zoom all
 * remount these boxes; none of them restart a shell.
 */
export function TerminalPane({ paneId, workspaceId, cwd, runtime, active }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    openTerminal(paneId, cwd, workspaceId, runtime);
    attachTerminal(paneId, host);
    const observer = new ResizeObserver(() => fitTerminal(paneId));
    observer.observe(host);
    return () => {
      observer.disconnect();
      detachTerminal(paneId);
    };
  }, [paneId, cwd, workspaceId, runtime]);

  // Declared after the attach effect, so it runs once the terminal is in place.
  useEffect(() => {
    if (active) focusTerminal(paneId);
  }, [active, paneId]);

  return <div className="terminal-pane" ref={hostRef} />;
}
