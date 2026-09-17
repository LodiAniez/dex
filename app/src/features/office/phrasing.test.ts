import { describe, expect, it } from "vitest";
import { chatLines, phrase } from "./phrasing";

const event = (kind: string, body: string, extra: Partial<{ seq: number; agent_id: string | null; author: string | null; key: string | null }> = {}) => ({
  seq: 1,
  agent_id: "a",
  author: "main",
  key: null,
  kind,
  body,
  ...extra,
});

const names = new Map([
  ["a", "Morgan"],
  ["b", "Pip"],
]);
const labels = new Map([["tests", "Pip"]]);
const who = { names, labels };

describe("phrase", () => {
  it("puts a note in its author's mouth", () => {
    expect(phrase(event("note", "14 retry tests green", { agent_id: "b" }), who)).toEqual({
      seq: 1,
      who: "Pip",
      text: "14 retry tests green",
      tone: "plain",
    });
  });

  it("credits a hire to HR: the event is the new agent's, and its body names who asked", () => {
    // The daemon records a spawn against the agent that was spawned.
    const hire = event("spawn", "main started an agent for: write the tests", { agent_id: "b", author: "tests" });
    expect(phrase(hire, { names, labels: new Map([["main", "Morgan"]]) })).toEqual({
      seq: 1,
      who: "HR",
      text: "hired Pip for Morgan: write the tests",
      tone: "good",
    });
  });

  it("says you, when it was you who hired them", () => {
    const hire = event("spawn", "you started an agent for: write the tests", { agent_id: "b" });
    expect(phrase(hire, who).text).toBe("hired Pip for you: write the tests");
  });

  it("keeps the label of a hirer the office cannot place", () => {
    const hire = event("spawn", "porter started an agent for: write the tests", { agent_id: "b" });
    expect(phrase(hire, who).text).toBe("hired Pip for porter: write the tests");
  });

  it("says what a status change means rather than what it is called", () => {
    expect(phrase(event("status", "main is waiting"), who)).toMatchObject({ who: "Morgan", text: "raised a hand — needs you", tone: "asks" });
    expect(phrase(event("status", "main is running"), who)).toMatchObject({ text: "back to work" });
    expect(phrase(event("status", "main is idle"), who)).toMatchObject({ text: "finished, waiting for mail" });
    expect(phrase(event("status", "main is error (rate_limit)"), who)).toMatchObject({ text: "stopped (rate_limit)", tone: "bad" });
    expect(phrase(event("status", "main is dead"), who)).toMatchObject({ text: "left the office" });
  });

  it("names a memo's recipient as the office does, keeping the label they are messaged by", () => {
    expect(phrase(event("message", "message to tests"), who)).toMatchObject({ who: "Morgan", text: "sent a memo to Pip (tests)" });
  });

  it("leaves a recipient it cannot place as it was written", () => {
    expect(phrase(event("message", "message to docs"), who)).toMatchObject({ text: "sent a memo to docs" });
  });

  it("says what was written or deleted", () => {
    expect(phrase(event("write", "set api.base", { key: "api.base" }), who)).toMatchObject({ text: "wrote api.base" });
    expect(phrase(event("delete", "removed api.base", { key: "api.base" }), who)).toMatchObject({ text: "deleted api.base" });
  });

  it("calls the human you", () => {
    expect(phrase(event("note", "ship it", { agent_id: null, author: null }), who).who).toBe("you");
  });

  it("falls back to the label for someone who has left, and to anything at all before nothing", () => {
    expect(phrase(event("note", "bye", { agent_id: "gone", author: "porter" }), who).who).toBe("porter");
    expect(phrase(event("note", "bye", { agent_id: "gone", author: null }), who).who).toBe("someone");
  });

  it("passes on a kind it has never heard of, unchanged", () => {
    expect(phrase(event("hologram", "shimmered"), who)).toMatchObject({ who: "Morgan", text: "shimmered" });
  });
});

describe("chatLines", () => {
  it("is the last few events, oldest first", () => {
    const log = [1, 2, 3, 4, 5].map((seq) => event("note", `n${seq}`, { seq }));
    expect(chatLines(log, who, 3).map((line) => line.text)).toEqual(["n3", "n4", "n5"]);
  });

  it("is empty before the log has loaded", () => {
    expect(chatLines(undefined, who, 3)).toEqual([]);
  });
});
