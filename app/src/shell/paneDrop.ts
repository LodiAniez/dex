/**
 * Dragging a pane by its header onto another: where it would land. Pure, so
 * the rule is tested without a DOM; `LayoutView` does the pointer work.
 */

/** Where a dropped pane lands against its target. Matches the daemon's `DropSide`. */
export type DropSide = "left" | "right" | "top" | "bottom" | "center";

/** How far into a pane, as a share of its size, counts as its edge. */
const EDGE = 0.25;
/** How far the pointer must travel before a pane lifts: a click on its header only focuses it. */
export const DRAG_START_PX = 6;

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface Point {
  x: number;
  y: number;
}

/** The side of `box` the pointer is over: the nearest edge within a quarter of it, else its middle. Null outside it. */
export function dropSide(box: Box, point: Point): DropSide | null {
  const fx = (point.x - box.left) / box.width;
  const fy = (point.y - box.top) / box.height;
  if (!(fx >= 0 && fx <= 1 && fy >= 0 && fy <= 1)) return null;
  const edges: [DropSide, number][] = [
    ["left", fx],
    ["right", 1 - fx],
    ["top", fy],
    ["bottom", 1 - fy],
  ];
  const [side, distance] = edges.reduce((nearest, edge) => (edge[1] < nearest[1] ? edge : nearest));
  return distance < EDGE ? side : "center";
}

/** Whether the pointer has moved far enough from where it went down to be a drag. */
export function isDrag(from: Point, to: Point): boolean {
  return Math.max(Math.abs(to.x - from.x), Math.abs(to.y - from.y)) >= DRAG_START_PX;
}

/**
 * What a key does while a pane is being dragged. Escape cancels. Once the pane
 * has lifted, no key goes on to the focused terminal: Escape there would
 * interrupt a Claude Code turn, and anything else would be typed into a pane
 * the owner is not looking at.
 */
export function dragKey(lifted: boolean, key: string): { cancel: boolean; swallow: boolean } {
  return { cancel: key === "Escape", swallow: lifted };
}

