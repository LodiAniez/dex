import { describe, expect, it } from "vitest";
import type { Layout } from "../platform/generated/Layout";
import { canPopOut, visibleActive, visibleLayout } from "./visibleLayout";

const leaf = (pane_id: string): Layout => ({ type: "leaf", pane_id });
const split = (dir: "horizontal" | "vertical", ratio: number, a: Layout, b: Layout): Layout => ({ type: "split", dir, ratio, a, b });

// p1 | (p2 over p3)
const tree = split("horizontal", 0.4, leaf("p1"), split("vertical", 0.5, leaf("p2"), leaf("p3")));

describe("visibleLayout", () => {
  it("is the whole tree when nothing is popped out, each split knowing where it is in the stored tree", () => {
    expect(visibleLayout(tree, new Set())).toEqual({
      type: "split",
      dir: "horizontal",
      ratio: 0.4,
      path: [],
      a: { type: "leaf", pane_id: "p1" },
      b: { type: "split", dir: "vertical", ratio: 0.5, path: ["b"], a: { type: "leaf", pane_id: "p2" }, b: { type: "leaf", pane_id: "p3" } },
    });
  });

  it("closes up round a popped-out pane, as if it had been closed, without touching the stored tree", () => {
    expect(visibleLayout(tree, new Set(["p2"]))).toEqual({
      type: "split",
      dir: "horizontal",
      ratio: 0.4,
      path: [],
      a: { type: "leaf", pane_id: "p1" },
      b: { type: "leaf", pane_id: "p3" },
    });
  });

  it("keeps the path of a split whose neighbour closed up, so dragging its divider changes the right split", () => {
    const shown = visibleLayout(tree, new Set(["p1"]));
    expect(shown).toMatchObject({ type: "split", dir: "vertical", path: ["b"] });
  });

  it("is nothing when every pane is out", () => {
    expect(visibleLayout(tree, new Set(["p1", "p2", "p3"]))).toBeNull();
  });
});

describe("canPopOut", () => {
  it("keeps at least one pane in the window", () => {
    expect(canPopOut(tree, new Set(), "p1")).toBe(true);
    expect(canPopOut(tree, new Set(["p2"]), "p1")).toBe(true);
    expect(canPopOut(tree, new Set(["p2", "p3"]), "p1")).toBe(false);
    expect(canPopOut(leaf("p1"), new Set(), "p1")).toBe(false);
  });

  it("does not pop out a pane that is already out", () => {
    expect(canPopOut(tree, new Set(["p2"]), "p2")).toBe(false);
  });
});

describe("visibleActive", () => {
  it("is the focused pane while it is in the window", () => {
    expect(visibleActive(tree, new Set(), "p2")).toBe("p2");
  });

  it("is never a pane in a window of its own: keys pressed here must not reach it, and closing it would kill its agent", () => {
    // p2 went out: its neighbour in the stack takes the focus.
    expect(visibleActive(tree, new Set(["p2"]), "p2")).toBe("p3");
    expect(visibleActive(tree, new Set(["p3"]), "p3")).toBe("p2");
    expect(visibleActive(tree, new Set(["p1"]), "p1")).toBe("p2");
  });

  it("is the first pane shown when none is recorded, and nothing when none is shown", () => {
    expect(visibleActive(tree, new Set(["p1"]), null)).toBe("p2");
    expect(visibleActive(tree, new Set(["p1", "p2", "p3"]), "p1")).toBeUndefined();
  });
});
