import type { Direction } from "./keybindings";

/**
 * The pane beside `from` in `direction`, judged by on-screen position: the
 * nearest pane that lies entirely on that side and overlaps it on the other
 * axis. Null at the edge. Panes are found by their `data-pane-id` attribute.
 */
export function neighborPane(from: string, direction: Direction): string | null {
  const panes = Array.from(document.querySelectorAll<HTMLElement>("[data-pane-id]")).map((el) => ({
    id: el.dataset.paneId ?? "",
    rect: el.getBoundingClientRect(),
  }));
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
