/**
 * Where things are on the floor map, in the map's own units. The map is one
 * SVG that scales to whatever pane it is in, so these never meet a pixel.
 */

import { PODS_PER_ROW } from "./floor";

export const MAP_WIDTH = 1280;
/** The building's wall is inset this far from the map's edge. */
export const WALL_INSET = 14;

/** A pod's box: name tag above, rug and desk below. */
export const POD = { width: 252, height: 230 } as const;

const FIRST_POD = { x: 340, y: 130 } as const;
const COLUMN_STEP = 310;
const ROW_STEP = 350;
const DESIGN_HEIGHT = 832;
const DESIGN_ROWS = 2;

/** Top-left of pod number `pod`, counted from 0 along the rows. */
export function podOrigin(pod: number): { x: number; y: number } {
  return {
    x: FIRST_POD.x + (pod % PODS_PER_ROW) * COLUMN_STEP,
    y: FIRST_POD.y + Math.floor(pod / PODS_PER_ROW) * ROW_STEP,
  };
}

/** The map's height for this many pods: the design's, plus a row's worth per row past two. */
export function mapHeight(pods: number): number {
  const rows = Math.max(DESIGN_ROWS, Math.ceil(pods / PODS_PER_ROW));
  return DESIGN_HEIGHT + (rows - DESIGN_ROWS) * ROW_STEP;
}

const FIRST_PLANK = 180;
const PLANK_STEP = 160;

/** The y of each horizontal plank line, top to bottom, stopping short of the wall. */
export function floorLines(height: number): number[] {
  const lines: number[] = [];
  for (let y = FIRST_PLANK; y < height - WALL_INSET - PLANK_STEP / 2; y += PLANK_STEP) lines.push(y);
  return lines;
}
