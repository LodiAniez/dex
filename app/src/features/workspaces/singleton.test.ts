import { describe, expect, it } from "vitest";
import { singletonTarget } from "./singleton";

const pane = (id: string, kind: string) => ({ id, kind });

describe("singletonTarget", () => {
  it("focuses the pane already showing it, because a second would show the same thing", () => {
    const ws = { active_pane: "p1", panes: [pane("p1", "terminal"), pane("p2", "office")] };
    expect(singletonTarget(ws, "office")).toEqual({ focus: "p2" });
  });

  it("splits one off the focused pane otherwise", () => {
    const ws = { active_pane: "p2", panes: [pane("p1", "terminal"), pane("p2", "terminal")] };
    expect(singletonTarget(ws, "office")).toEqual({ split: "p2" });
  });

  it("splits off the first pane when nothing is focused", () => {
    const ws = { active_pane: null, panes: [pane("p1", "terminal")] };
    expect(singletonTarget(ws, "activity")).toEqual({ split: "p1" });
  });

  it("does not confuse one singleton with another", () => {
    const ws = { active_pane: "p1", panes: [pane("p1", "activity")] };
    expect(singletonTarget(ws, "office")).toEqual({ split: "p1" });
  });

  it("has nothing to do in a workspace with no panes", () => {
    expect(singletonTarget({ active_pane: null, panes: [] }, "office")).toBeNull();
  });
});
