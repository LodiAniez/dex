import { useEffect, useRef } from "react";
import { attachTerminal, detachTerminal, fitTerminal, openTerminal } from "../../platform/terminalRegistry";

/**
 * A layout box that shows one pane's terminal. Mounting attaches the terminal;
 * unmounting only detaches it — the terminal and its PTY live on in the
 * registry until the pane is closed (PRD §7.3).
 */
export function TerminalPane({ paneId, cwd }: { paneId: string; cwd?: string }) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    openTerminal(paneId, cwd);
    attachTerminal(paneId, host);
    const observer = new ResizeObserver(() => fitTerminal(paneId));
    observer.observe(host);
    return () => {
      observer.disconnect();
      detachTerminal(paneId);
    };
  }, [paneId, cwd]);

  return <div className="terminal-pane" ref={hostRef} />;
}
