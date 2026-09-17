import { describe, expect, it } from "vitest";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { announcePlan, announceSummary, promptArgs, whyNoPrompt } from "./prompt";

const member = (name: string, status: AgentStatus, pane: string | null = `pane-${name}`) => ({
  agent: { id: `id-${name}`, status, pane_id: pane },
  persona: { name },
});

describe("promptArgs", () => {
  it("types the prompt into the agent's terminal and presses Enter", () => {
    expect(promptArgs("p1", "  run the tests again  ")).toEqual({ pane: "p1", text: "run the tests again", enter: true });
  });

  it("keeps a prompt on one line, because a newline in a terminal is Enter", () => {
    // Half a prompt submitted early is worse than a prompt with its breaks flattened.
    expect(promptArgs("p1", "first do this\nthen that\r\n\r\nand report")?.text).toBe("first do this then that and report");
  });

  it("has nothing to send when nothing was written", () => {
    expect(promptArgs("p1", " \n ")).toBeNull();
  });
});

describe("whyNoPrompt", () => {
  it("lets an idle or a working agent be prompted", () => {
    expect(whyNoPrompt(member("a", "idle").agent)).toBeNull();
    expect(whyNoPrompt(member("a", "running").agent)).toBeNull();
  });

  it("refuses one waiting on a dialog, where typed text would answer the dialog", () => {
    expect(whyNoPrompt(member("a", "waiting").agent)).toMatch(/waiting for you/i);
  });

  it("refuses one with no pane to type into", () => {
    expect(whyNoPrompt(member("a", "idle", null).agent)).toMatch(/no pane/i);
  });
});

describe("announcePlan", () => {
  const staff = [member("Morgan", "running"), member("Juno", "waiting"), member("Pip", "idle"), member("Ravi", "idle", null)];

  it("reaches everyone who can be prompted and names everyone who cannot", () => {
    const plan = announcePlan(staff);
    expect(plan.to.map((m) => m.persona.name)).toEqual(["Morgan", "Pip"]);
    expect(plan.skipped.map((m) => m.persona.name)).toEqual(["Juno", "Ravi"]);
  });

  it("says who heard it, and who did not and why", () => {
    expect(announceSummary(announcePlan(staff))).toBe("Announced to 2 agents. Not to Juno (waiting for you) or Ravi (no pane).");
    expect(announceSummary(announcePlan([member("Pip", "idle")]))).toBe("Announced to 1 agent.");
    expect(announceSummary(announcePlan([member("Juno", "waiting")]))).toBe("Nobody to announce to. Not to Juno (waiting for you).");
  });
});
