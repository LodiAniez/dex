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

/** The first pane `layout` shows, left to right, top to bottom, leaving out `detached`. */
function firstShown(layout: Layout, detached: ReadonlySet<string>): string | undefined {
  if (layout.type === "leaf") return detached.has(layout.pane_id) ? undefined : layout.pane_id;
  return firstShown(layout.a, detached) ?? firstShown(layout.b, detached);
}

/** The subtrees from the root down to `paneId`'s leaf, or null if it is not in `layout`. */
function lineage(layout: Layout, paneId: string): Layout[] | null {
  if (layout.type === "leaf") return layout.pane_id === paneId ? [layout] : null;
  for (const child of [layout.a, layout.b]) {
    const below = lineage(child, paneId);
    if (below) return [layout, ...below];
  }
  return null;
}

/**
 * The pane the main window treats as focused: `active` while it is in the
 * window, else the nearest pane that is - its neighbour in the tree first. A
 * pane in a window of its own must never be the focus here: keys pressed would
 * go nowhere, and pane shortcuts would act on it unseen - closing it would kill
 * its agent.
 */
export function visibleActive(layout: Layout, detached: ReadonlySet<string>, active: string | null | undefined): string | undefined {
  const line = active ? lineage(layout, active) : null;
  if (!line) return firstShown(layout, detached);
  if (!detached.has(active as string)) return active as string;
  // Up from the pane, looking on the other side of each split.
  for (let depth = line.length - 2; depth >= 0; depth -= 1) {
    const split = line[depth];
    if (split.type !== "split") continue;
    const other = split.a === line[depth + 1] ? split.b : split.a;
    const found = firstShown(other, detached);
    if (found) return found;
  }
  return undefined;
}
