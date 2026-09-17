import { describe, expect, it } from "vitest";
import { clockOutArgs, clockOutQuestion, hireArgs, isHiringFreeze, labelClash, memoArgs } from "./hire";

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

describe("clocking out", () => {
  it("asks Claude Code to leave rather than killing it, and takes the pane too", () => {
    expect(clockOutArgs("agent-7")).toEqual({ agent: "agent-7", graceful: true, close_pane: true });
  });

  it("says, before anything happens, what cannot be undone", () => {
    const question = clockOutQuestion("Pip", "tests");
    expect(question).toContain("Pip (tests)");
    expect(question).toContain("/exit");
    expect(question).toContain("pane");
  });
});

describe("labelClash", () => {
  const panes = [
    { id: "p1", label: "lead" },
    { id: "p2", label: "porter" },
    { id: "p3", label: "lead-2" },
    { id: "p4", label: null },
  ];
  const withAgents = new Set(["p2"]);

  it("is nothing for a label nobody holds, or for no label at all", () => {
    expect(labelClash(panes, "tests", withAgents)).toBeNull();
    expect(labelClash(panes, "  ", withAgents)).toBeNull();
  });

  it("says a pane with no agent in it holds the label: from the office that pane cannot be seen", () => {
    // The bug report: "lead is already used", with no agents anywhere in sight.
    const clash = labelClash(panes, " lead ", withAgents);
    expect(clash?.why).toBe('A pane labelled "lead" is still open in Terminal view, with no agent in it. Labels belong to panes.');
  });

  it("says an agent holds it when one does", () => {
    expect(labelClash(panes, "porter", withAgents)?.why).toBe(`An agent's pane is already labelled "porter".`);
  });

  it("offers the next free label of the same name", () => {
    expect(labelClash(panes, "lead", withAgents)?.free).toBe("lead-3");
    expect(labelClash(panes, "porter", withAgents)?.free).toBe("porter-2");
  });
});
