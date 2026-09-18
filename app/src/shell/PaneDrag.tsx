import "./paneDrag.css";
import { createContext, useCallback, useContext, useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { movePane } from "../features/workspaces";
import { showError } from "../platform/notices";
import { dragKey, dropSide, isDrag, type DropSide } from "./paneDrop";

/** A pane being dragged by its header, and where it would land if let go now. */
export interface Drag {
  moving: string;
  target: string | null;
  side: DropSide | null;
}

interface PaneDragApi {
  drag: Drag | null;
  /** Starts watching a pointer that went down on a pane's header. */
  begin: (paneId: string, event: PointerEvent<HTMLElement>) => void;
}

const PaneDragContext = createContext<PaneDragApi>({ drag: null, begin: () => {} });

export function usePaneDrag(): PaneDragApi {
  return useContext(PaneDragContext);
}

/**
 * Drag a pane by its header and drop it on another: on an edge to sit beside
 * it there, in the middle to trade places (issue #31). A click on a header still
 * only focuses the pane - it lifts after a few pixels - and Escape, or letting
 * go anywhere but on another pane, puts it back. The header holds the pointer
 * until then, so letting go outside the window is seen too.
 */
export function PaneDragArea({ children }: { children: ReactNode }) {
  const [drag, setDrag] = useState<Drag | null>(null);
  const cleanup = useRef<(() => void) | null>(null);
  useEffect(() => () => cleanup.current?.(), []);

  const begin = useCallback((paneId: string, down: PointerEvent<HTMLElement>) => {
    if (down.button !== 0) return;
    cleanup.current?.();
    const start = { x: down.clientX, y: down.clientY };
    // Held by the header, so the release is seen wherever it happens - even outside the window.
    const handle = down.currentTarget;
    const pointer = down.pointerId;
    try {
      handle.setPointerCapture(pointer);
    } catch {
      // The pointer is already gone; the window listeners still end the drag.
    }
    let lifted = false;
    let latest: Drag = { moving: paneId, target: null, side: null };

    const onMove = (move: globalThis.PointerEvent) => {
      const point = { x: move.clientX, y: move.clientY };
      if (!lifted) {
        if (!isDrag(start, point)) return;
        lifted = true;
        document.body.classList.add("pane-dragging");
      }
      const over = document.elementFromPoint(point.x, point.y)?.closest<HTMLElement>("[data-pane-id]");
      const target = over?.dataset.paneId ?? null;
      const side = over && target !== paneId ? dropSide(over.getBoundingClientRect(), point) : null;
      latest = { moving: paneId, target: side ? target : null, side };
      setDrag(latest);
    };
    const finish = (drop: boolean) => {
      cleanup.current?.();
      if (drop && latest.target && latest.side) void movePane(latest.moving, latest.target, latest.side).catch(showError);
    };
    const onUp = () => finish(lifted);
    const onCancel = () => finish(false);
    const onKey = (key: KeyboardEvent) => {
      const { cancel, swallow } = dragKey(lifted, key.key);
      if (swallow) {
        key.preventDefault();
        key.stopImmediatePropagation();
      }
      if (cancel) finish(false);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    handle.addEventListener("lostpointercapture", onUp);
    window.addEventListener("keydown", onKey, true);
    cleanup.current = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      handle.removeEventListener("lostpointercapture", onUp);
      if (handle.hasPointerCapture(pointer)) handle.releasePointerCapture(pointer);
      window.removeEventListener("keydown", onKey, true);
      document.body.classList.remove("pane-dragging");
      cleanup.current = null;
      setDrag(null);
    };
  }, []);

  return <PaneDragContext.Provider value={{ drag, begin }}>{children}</PaneDragContext.Provider>;
}

/** Where on this pane a dragged pane would land, drawn over it. */
export function DropHint({ side }: { side: DropSide }) {
  return <div className={`pane-drop-hint ${side}`} aria-hidden="true" />;
}
