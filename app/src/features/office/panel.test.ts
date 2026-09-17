import { describe, expect, it } from "vitest";
import { eventsOf, reportsTo } from "./panel";

const event = (seq: number, agent_id: string | null, body = `event ${seq}`) => ({ seq, agent_id, body });

describe("eventsOf", () => {
  const log = [event(1, "a"), event(2, null), event(3, "b"), event(4, "a"), event(5, "a"), event(6, "b")];

  it("is what one agent did, by id, oldest first", () => {
    expect(eventsOf(log, "a", 10).map((e) => e.seq)).toEqual([1, 4, 5]);
  });

  it("keeps the newest when there are more than fit", () => {
    expect(eventsOf(log, "a", 2).map((e) => e.seq)).toEqual([4, 5]);
  });

  it("never shows the human's events as an agent's", () => {
    expect(eventsOf(log, "nobody", 10)).toEqual([]);
  });

  it("copes with a log that has not loaded", () => {
    expect(eventsOf(undefined, "a", 5)).toEqual([]);
  });
});

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
