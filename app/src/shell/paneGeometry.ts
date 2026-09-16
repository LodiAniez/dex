import type { Direction } from "./keybindings";

/** A pane's box on screen. `DOMRect` satisfies this. */
export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

/** A pane and where it is. */
export interface PaneBox {
  id: string;
  rect: Box;
}

/**
 * The pane beside `from` in `direction`, judged by on-screen position: the
 * nearest pane that lies entirely on that side and overlaps it on the other
 * axis. Null at the edge.
 *
 * Split from the DOM lookup so the geometry can be tested against boxes. A
 * headless DOM would not help: `getBoundingClientRect` returns zeros there, so
 * every pane would sit on top of every other.
 */
export function neighborIn(panes: PaneBox[], from: string, direction: Direction): string | null {
  const origin = panes.find((pane) => pane.id === from)?.rect;
  if (!origin) return null;

  const centerX = origin.left + origin.width / 2;
  const centerY = origin.top + origin.height / 2;
  const horizontal = direction === "left" || direction === "right";
  let best: { id: string; distance: number } | null = null;

  for (const { id, rect } of panes) {
    if (id === from) continue;
    // A pixel of slack: adjacent panes share an edge, give or take the divider.
    const beside =
      direction === "right" ? rect.left >= origin.right - 1
      : direction === "left" ? rect.right <= origin.left + 1
      : direction === "down" ? rect.top >= origin.bottom - 1
      : rect.bottom <= origin.top + 1;
    const overlaps = horizontal
      ? rect.top < origin.bottom && rect.bottom > origin.top
      : rect.left < origin.right && rect.right > origin.left;
    if (!beside || !overlaps) continue;
    const distance = Math.hypot(rect.left + rect.width / 2 - centerX, rect.top + rect.height / 2 - centerY);
    if (!best || distance < best.distance) best = { id, distance };
  }
  return best?.id ?? null;
}

/** Every pane on screen, found by its `data-pane-id` attribute. */
function panesOnScreen(): PaneBox[] {
  return Array.from(document.querySelectorAll<HTMLElement>("[data-pane-id]")).map((el) => ({
    id: el.dataset.paneId ?? "",
    rect: el.getBoundingClientRect(),
  }));
}

/** The pane beside `from` in `direction`, as the window currently shows them. */
export function neighborPane(from: string, direction: Direction): string | null {
  return neighborIn(panesOnScreen(), from, direction);
}
