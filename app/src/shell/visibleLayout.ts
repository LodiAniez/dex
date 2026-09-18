/**
 * The layout as the main window shows it: panes popped out into windows of
 * their own are left out, and the space closes up round them - as if they had
 * been closed - without the stored tree changing. Docking one back puts it
 * where it was. Each split keeps its path in the stored tree, so dragging its
 * divider changes the right split.
 */

import type { Layout } from "../platform/generated/Layout";

export type Side = "a" | "b";

export type Shown =
  | { type: "leaf"; pane_id: string }
  | { type: "split"; dir: "horizontal" | "vertical"; ratio: number; a: Shown; b: Shown; path: Side[] };

export function visibleLayout(layout: Layout, detached: ReadonlySet<string>, path: Side[] = []): Shown | null {
  if (layout.type === "leaf") return detached.has(layout.pane_id) ? null : { type: "leaf", pane_id: layout.pane_id };
  const a = visibleLayout(layout.a, detached, [...path, "a"]);
  const b = visibleLayout(layout.b, detached, [...path, "b"]);
  if (!a) return b;
  if (!b) return a;
  return { type: "split", dir: layout.dir, ratio: layout.ratio, a, b, path };
}

/** Whether `paneId` may be popped out: it is in the window, and it is not the last pane there. */
export function canPopOut(layout: Layout, detached: ReadonlySet<string>, paneId: string): boolean {
  if (detached.has(paneId)) return false;
  const shown = visibleLayout(layout, detached);
  return shown !== null && shown.type === "split";
}
