import { describe, expect, it } from "vitest";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { announcePlan, announceSummary, promptArgs, whyNoPrompt } from "./prompt";

const member = (name: string, status: AgentStatus, pane: string | null = `pane-${name}`, started = true) => ({
  agent: { id: `id-${name}`, status, pane_id: pane, started },
  persona: { name },
});

describe("promptArgs", () => {
  it("asks the daemon to prompt the agent, which is where it is decided whether that is safe", () => {
    expect(promptArgs("agent-1", "  run the tests again  ")).toEqual({ agent: "agent-1", text: "run the tests again" });
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

  it("refuses a hire whose Claude Code has not started: its pane is a bare shell that would run the text", () => {
    expect(whyNoPrompt(member("a", "idle", "p", false).agent)).toMatch(/not started/i);
  });

  it("refuses one that has stopped or gone quiet, where Claude Code may not be there", () => {
    expect(whyNoPrompt(member("a", "error").agent)).toMatch(/may not be running/i);
    expect(whyNoPrompt(member("a", "unknown").agent)).toMatch(/may not be running/i);
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
