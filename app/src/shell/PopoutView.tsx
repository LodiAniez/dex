import "./popout.css";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef, useState } from "react";
import { attachTerminal, fitTerminal, focusTerminal, handOff, takeOver } from "../platform/terminalRegistry";
import { DOCK, DOCKED, READY, STATE, type PaneMessage, type Screen } from "./popouts";

/** How long to wait for the main window to take a docked pane back before closing anyway. */
const DOCK_WAIT_MS = 4000;

/** Which pane this window shows, if it is a pane's own window (set by src-tauri/src/popout.rs). */
export function popoutOfThisWindow(): { paneId: string; title: string } | null {
  const told = (window as unknown as { __DEX_POPOUT__?: { paneId?: unknown; title?: unknown } }).__DEX_POPOUT__;
  if (!told || typeof told.paneId !== "string") return null;
  return { paneId: told.paneId, title: typeof told.title === "string" ? told.title : "pane" };
}

/**
 * A pane in a window of its own: its terminal, and a way back. Closing the
 * window docks the pane too - nothing is lost either way.
 */
export function PopoutView({ paneId, title }: { paneId: string; title: string }) {
  const host = useRef<HTMLDivElement>(null);
  const [ready, setReady] = useState(false);
  // Read by the close handler, which is registered once for the window's life.
  const readyNow = useRef(false);
  readyNow.current = ready;
  const [docking, setDocking] = useState(false);
  const leaving = useRef(false);

  useEffect(() => {
    document.title = `${title} - Dex`;
    let cancelled = false;
    const stops: (() => void)[] = [];
    void (async () => {
      stops.push(
        await listen<Screen>(STATE, (event) => {
          if (event.payload.paneId !== paneId || cancelled) return;
          void takeOver(paneId, event.payload.content).then(() => setReady(true));
        }),
      );
      await emitTo("main", READY, { paneId } satisfies PaneMessage);
    })();
    return () => {
      cancelled = true;
      stops.forEach((stop) => stop());
    };
  }, [paneId, title]);

  useEffect(() => {
    const element = host.current;
    if (!ready || !element) return;
    attachTerminal(paneId, element);
    focusTerminal(paneId);
    const observer = new ResizeObserver(() => fitTerminal(paneId));
    observer.observe(element);
    return () => observer.disconnect();
  }, [ready, paneId]);

  const dock = async () => {
    if (leaving.current) return;
    leaving.current = true;
    setDocking(true);
    const me = getCurrentWindow();
    const done = new Promise<void>((resolve) => {
      const timer = window.setTimeout(resolve, DOCK_WAIT_MS);
      void listen<PaneMessage>(DOCKED, (event) => {
        if (event.payload.paneId !== paneId) return;
        window.clearTimeout(timer);
        resolve();
      });
    });
    const content = readyNow.current ? await handOff(paneId) : null;
    if (content !== null) await emitTo("main", DOCK, { paneId, content } satisfies Screen);
    await done;
    await me.destroy();
  };

  // The window's own close button docks the pane rather than losing its screen.
  // Registered once: a second registration still in flight when the first was
  // removed could win the race and close the window without handing back.
  const dockRef = useRef(dock);
  dockRef.current = dock;
  useEffect(() => {
    let cancelled = false;
    let stop: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        event.preventDefault();
        void dockRef.current();
      })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  return (
    <div className="popout">
      <div className="popout-header">
        <span className="popout-title">{title}</span>
        <button type="button" className="popout-dock" disabled={docking} onClick={() => void dock()} title="Put this pane back in the Dex window">
          {docking ? "Docking…" : "Dock ↩"}
        </button>
      </div>
      <div className="popout-body" ref={host}>
        {!ready && <div className="popout-wait">Moving the terminal here…</div>}
      </div>
    </div>
  );
}
