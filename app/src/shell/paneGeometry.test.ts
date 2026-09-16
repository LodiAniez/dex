import { describe, expect, it } from "vitest";
import { neighborIn, type PaneBox } from "./paneGeometry";

/** A pane box from its edges, the way a layout would produce one. */
function box(id: string, left: number, top: number, right: number, bottom: number): PaneBox {
  return {
    id,
    rect: { left, top, right, bottom, width: right - left, height: bottom - top },
  };
}

/**
 *  ┌──────────┬──────────┐
 *  │          │    tr     │
 *  │   left   ├──────────┤
 *  │          │    br     │
 *  └──────────┴──────────┘
 */
const splitRight: PaneBox[] = [
  box("left", 0, 0, 500, 800),
  box("tr", 500, 0, 1000, 400),
  box("br", 500, 400, 1000, 800),
];

describe("finding the pane beside another", () => {
  it("returns null at the edge of the layout", () => {
    expect(neighborIn(splitRight, "left", "left")).toBeNull();
    expect(neighborIn(splitRight, "tr", "up")).toBeNull();
    expect(neighborIn(splitRight, "br", "down")).toBeNull();
    expect(neighborIn(splitRight, "tr", "right")).toBeNull();
  });

  it("returns null for a pane that is not on screen", () => {
    expect(neighborIn(splitRight, "gone", "right")).toBeNull();
  });

  it("returns null when there is nothing but the pane itself", () => {
    expect(neighborIn([box("only", 0, 0, 100, 100)], "only", "right")).toBeNull();
  });

  it("moves between stacked panes", () => {
    expect(neighborIn(splitRight, "tr", "down")).toBe("br");
    expect(neighborIn(splitRight, "br", "up")).toBe("tr");
  });

  it("takes the nearest of several candidates", () => {
    // `left` faces both right-hand panes; their centres are the same distance
    // away, so this is really about it choosing one and not erroring.
    expect(["tr", "br"]).toContain(neighborIn(splitRight, "left", "right"));
  });

  it("prefers the pane whose centre is closest", () => {
    //  left spans the full height; `near` is level with its centre, `far` is not.
    const panes = [
      box("left", 0, 0, 500, 800),
      box("far", 500, 0, 1000, 200),
      box("near", 500, 300, 1000, 500),
    ];
    expect(neighborIn(panes, "left", "right")).toBe("near");
  });

  it("ignores a pane on the right that does not overlap vertically", () => {
    // Directly right means beside, not diagonally past a corner.
    const panes = [box("here", 0, 0, 500, 400), box("below-right", 500, 400, 1000, 800)];
    expect(neighborIn(panes, "here", "right")).toBeNull();
  });

  it("ignores a pane below that does not overlap horizontally", () => {
    const panes = [box("here", 0, 0, 500, 400), box("right-below", 500, 400, 1000, 800)];
    expect(neighborIn(panes, "here", "down")).toBeNull();
  });

  it("treats panes sharing a divider as adjacent", () => {
    // A divider leaves a gap of a pixel or so; the slack in the test covers it.
    const panes = [box("a", 0, 0, 499, 800), box("b", 500, 0, 1000, 800)];
    expect(neighborIn(panes, "a", "right")).toBe("b");
    expect(neighborIn(panes, "b", "left")).toBe("a");
  });

  it("does not mistake an overlapping pane for a neighbour", () => {
    // Nothing should produce this, but a pane on top of another must not count
    // as being to its right, or Alt+Arrow would jump to a hidden pane.
    const panes = [box("a", 0, 0, 500, 800), box("on-top", 100, 100, 400, 700)];
    expect(neighborIn(panes, "a", "right")).toBeNull();
    expect(neighborIn(panes, "a", "down")).toBeNull();
  });

  it("walks a row of three one step at a time", () => {
    const row = [box("a", 0, 0, 300, 800), box("b", 300, 0, 600, 800), box("c", 600, 0, 900, 800)];
    expect(neighborIn(row, "a", "right")).toBe("b");
    expect(neighborIn(row, "b", "right")).toBe("c");
    expect(neighborIn(row, "c", "right")).toBeNull();
    expect(neighborIn(row, "c", "left")).toBe("b");
    expect(neighborIn(row, "b", "left")).toBe("a");
  });
});
