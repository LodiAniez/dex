import { describe, expect, it } from "vitest";
import { hireArgs, isHiringFreeze, memoArgs } from "./hire";

const pane = (id: string, kind: string) => ({ id, kind });

describe("hireArgs", () => {
  const ws = { id: "ws-1", active_pane: "p2", panes: [pane("p1", "terminal"), pane("p2", "terminal"), pane("p3", "office")] };

  it("splits the new agent off the focused terminal", () => {
    expect(hireArgs(ws, "  port the auth module  ", "")).toEqual({ task: "port the auth module", pane: "p2", workspace: "ws-1" });
  });

  it("does not split the office itself, which is usually what has focus", () => {
    expect(hireArgs({ ...ws, active_pane: "p3" }, "do it", "")?.pane).toBe("p1");
  });

  it("passes a label only when one was typed", () => {
    expect(hireArgs(ws, "do it", " tests ")).toMatchObject({ label: "tests" });
    expect(hireArgs(ws, "do it", "   ")).not.toHaveProperty("label");
  });

  it("names the workspace alone when it has no terminal to split", () => {
    const args = hireArgs({ id: "ws-1", active_pane: "p3", panes: [pane("p3", "office")] }, "do it", "");
    expect(args).toEqual({ task: "do it", workspace: "ws-1" });
  });

  it("has nothing to send without a task", () => {
    expect(hireArgs(ws, "   ", "tests")).toBeNull();
  });
});

describe("isHiringFreeze", () => {
  it("is the daemon refusing for want of a seat, and nothing else", () => {
    expect(isHiringFreeze({ code: "concurrency_limit" })).toBe(true);
    expect(isHiringFreeze({ code: "depth_limit" })).toBe(false);
    expect(isHiringFreeze(new Error("pipe closed"))).toBe(false);
    expect(isHiringFreeze(null)).toBe(false);
  });
});

describe("memoArgs", () => {
  it("addresses the agent by id, which no other agent can share", () => {
    expect(memoArgs("ws-1", "agent-7", " the schema moved ")).toEqual({
      target_agent: "agent-7",
      body: "the schema moved",
      workspace: "ws-1",
    });
  });

  it("has nothing to send when nothing was written", () => {
    expect(memoArgs("ws-1", "agent-7", " \n ")).toBeNull();
  });
});
