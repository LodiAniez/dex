import { describe, expect, it } from "vitest";
import { ago, doneBy, reportsTo } from "./panel";

describe("reportsTo", () => {
  const staff = [
    { agent: { id: "boss", parent_id: null }, persona: { name: "Morgan" } },
    { agent: { id: "kid", parent_id: "boss" }, persona: { name: "Pip" } },
    { agent: { id: "orphan", parent_id: "left" }, persona: { name: "Ravi" } },
  ];

  it("is you, for an agent you started", () => {
    expect(reportsTo(staff[0], staff)).toBe("you");
  });

  it("is the parent, by the name the office knows them by", () => {
    expect(reportsTo(staff[1], staff)).toBe("Morgan");
  });

  it("says so when the parent has gone", () => {
    expect(reportsTo(staff[2], staff)).toBe("someone who has left");
  });
});

describe("doneBy", () => {
  const did = (seq: number, agent_id: string | null, kind: string) => ({ seq, agent_id, kind, body: `${kind} ${seq}` });
  const log = [did(1, "a", "spawn"), did(2, "a", "status"), did(3, "a", "note"), did(4, "b", "note"), did(5, "a", "status"), did(6, "a", "write"), did(7, "a", "message"), did(8, null, "note")];

  it("is everything the agent did, newest first", () => {
    expect(doneBy(log, "a").map((e) => e.seq)).toEqual([7, 6, 3, 1]);
  });

  it("leaves out status flips, which say what state it was in and not what it did", () => {
    expect(doneBy(log, "a").some((e) => e.kind === "status")).toBe(false);
  });

  it("counts a hire as the hirer's doing, though the daemon files it under the one hired", () => {
    // A spawn event belongs to the new agent; its body names who asked.
    const hires = [
      { seq: 1, agent_id: "kid", kind: "spawn", body: "main started an agent for: write the tests" },
      { seq: 2, agent_id: "other", kind: "spawn", body: "porter started an agent for: review it" },
      { seq: 3, agent_id: "lead", kind: "note", body: "briefed everyone" },
    ];
    expect(doneBy(hires, "lead", ["main"]).map((e) => e.seq)).toEqual([3, 1]);
    // And it is still the first thing on the record of the one who was hired.
    expect(doneBy(hires, "kid", ["tests"]).map((e) => e.seq)).toEqual([1]);
  });

  it("is not fooled by a label that only begins the same way", () => {
    const hires = [{ seq: 1, agent_id: "kid", kind: "spawn", body: "main-2 started an agent for: x" }];
    expect(doneBy(hires, "lead", ["main"])).toEqual([]);
  });

  it("is empty for someone who has done nothing yet, or a log not loaded", () => {
    expect(doneBy(log, "nobody")).toEqual([]);
    expect(doneBy(undefined, "a")).toEqual([]);
  });
});

describe("ago", () => {
  const now = 1_000_000_000;
  it("says how long ago in the largest unit that fits", () => {
    expect(ago(now - 4_000, now)).toBe("just now");
    expect(ago(now - 45_000, now)).toBe("45s ago");
    expect(ago(now - 5 * 60_000, now)).toBe("5m ago");
    expect(ago(now - 3 * 3_600_000, now)).toBe("3h ago");
    expect(ago(now - 2 * 86_400_000, now)).toBe("2d ago");
  });

  it("does not go negative when clocks disagree", () => {
    expect(ago(now + 5_000, now)).toBe("just now");
  });
});
