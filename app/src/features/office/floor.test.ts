import { describe, expect, it } from "vitest";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { headcount, hrNote, occupants, podCount, poseOf, seat, type Seating } from "./floor";

const agent = (id: string, extra: Partial<{ workspace_id: string; status: AgentStatus; started_at: number }> = {}) => ({
  id,
  workspace_id: "ws",
  status: "running" as AgentStatus,
  started_at: 0,
  ...extra,
});

describe("occupants", () => {
  it("is the living agents of one workspace, in the order they arrived", () => {
    const all = [
      agent("late", { started_at: 30 }),
      agent("elsewhere", { workspace_id: "other", started_at: 5 }),
      agent("gone", { status: "dead", started_at: 1 }),
      agent("early", { started_at: 10 }),
    ];
    expect(occupants(all, "ws").map((a) => a.id)).toEqual(["early", "late"]);
  });

  it("breaks a tie on the id, so two views agree", () => {
    const all = [agent("b", { started_at: 1 }), agent("a", { started_at: 1 })];
    expect(occupants(all, "ws").map((a) => a.id)).toEqual(["a", "b"]);
  });

  it("keeps an agent nobody can reach at its desk", () => {
    expect(occupants([agent("lost", { status: "unknown" })], "ws")).toHaveLength(1);
  });
});

describe("seat", () => {
  const empty: Seating = new Map();

  it("fills an empty office from the first pod", () => {
    const seating = seat(empty, [agent("a"), agent("b"), agent("c")]);
    expect([...seating]).toEqual([["a", 0], ["b", 1], ["c", 2]]);
  });

  it("leaves everyone where they are when the list comes back reordered", () => {
    const before = seat(empty, [agent("a"), agent("b"), agent("c")]);
    const after = seat(before, [agent("c"), agent("a"), agent("b")]);
    expect(after.get("a")).toBe(0);
    expect(after.get("b")).toBe(1);
    expect(after.get("c")).toBe(2);
  });

  it("does not shuffle the office when someone leaves", () => {
    const before = seat(empty, [agent("a"), agent("b"), agent("c")]);
    const after = seat(before, [agent("a"), agent("c")]);
    expect(after.has("b")).toBe(false);
    expect(after.get("c")).toBe(2);
  });

  it("gives a newcomer the first vacant pod", () => {
    const before = seat(empty, [agent("a"), agent("b"), agent("c")]);
    const vacated = seat(before, [agent("a"), agent("c")]);
    const after = seat(vacated, [agent("a"), agent("c"), agent("d")]);
    expect(after.get("d")).toBe(1);
  });

  it("seats several newcomers in the order given, never two to a pod", () => {
    const before = seat(empty, [agent("a")]);
    const after = seat(before, [agent("a"), agent("x"), agent("y")]);
    expect(after.get("x")).toBe(1);
    expect(after.get("y")).toBe(2);
    expect(new Set(after.values()).size).toBe(after.size);
  });

  it("does not touch the seating it was given", () => {
    const before = seat(empty, [agent("a")]);
    seat(before, [agent("a"), agent("b")]);
    expect([...before]).toEqual([["a", 0]]);
  });
});

describe("podCount", () => {
  it("is two rows of three for the default limit", () => {
    expect(podCount(new Map(), 6)).toBe(6);
  });

  it("never shows fewer than two rows, however low the limit", () => {
    expect(podCount(new Map(), 1)).toBe(6);
  });

  it("adds a whole row when the limit is raised", () => {
    expect(podCount(new Map(), 7)).toBe(9);
    expect(podCount(new Map(), 9)).toBe(9);
  });

  it("keeps a row for someone seated beyond a limit that was lowered", () => {
    expect(podCount(new Map([["a", 7]]), 6)).toBe(9);
  });
});

describe("headcount", () => {
  it("counts seats against the limit", () => {
    expect(headcount(4, 6)).toEqual({ used: 4, max: 6, full: false, text: "4 / 6 seats" });
  });

  it("says when the office is full", () => {
    expect(headcount(6, 6)).toMatchObject({ full: true, text: "Office full · 6 / 6" });
  });

  it("is full, not broken, when a lowered limit leaves it over", () => {
    expect(headcount(7, 6)).toMatchObject({ full: true, text: "Office full · 7 / 6" });
  });
});

describe("poseOf", () => {
  it("has a pose for every status an occupant can be in", () => {
    const poses: Record<Exclude<AgentStatus, "dead">, string> = {
      running: "typing",
      waiting: "raised",
      idle: "still",
      error: "error",
      unknown: "still",
    };
    for (const [status, pose] of Object.entries(poses)) {
      expect(poseOf(status as AgentStatus), status).toBe(pose);
    }
  });
});

describe("hrNote", () => {
  const vacancy = [{}, null, {}, null];

  it("points at the first free office", () => {
    expect(hrNote(headcount(2, 6), vacancy, false)).toBe("Office 2 is free.");
  });

  it("says why nobody can be hired when the office is full", () => {
    expect(hrNote(headcount(6, 6), [{}, {}], false)).toBe("No seat until someone leaves.");
  });

  it("calls a refused hire what it is, until a seat frees up", () => {
    expect(hrNote(headcount(6, 6), [{}, {}], true)).toBe("Hiring freeze — every seat is taken.");
    expect(hrNote(headcount(5, 6), vacancy, true)).toBe("Office 2 is free.");
  });
});
