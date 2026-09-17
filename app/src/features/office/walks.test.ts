import { describe, expect, it } from "vitest";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { HR_DOOR, enqueue, legsTo, movements, routeOf, walkDuration, type Walk } from "./walks";

const agent = (id: string, extra: Partial<{ status: AgentStatus; parent_id: string | null; depth: number; workspace_id: string }> = {}) => ({
  id,
  workspace_id: "ws",
  status: "running" as AgentStatus,
  parent_id: null,
  depth: 0,
  ...extra,
});

describe("movements", () => {
  it("sends someone hired by another agent in through HR", () => {
    const before = [agent("boss")];
    const after = [agent("boss"), agent("kid", { parent_id: "boss", depth: 1 })];
    expect(movements(before, after, "ws")).toEqual([{ kind: "arrive", id: "kid" }]);
  });

  it("sends someone a human hired through HR in the same way", () => {
    // Spawned from a pane with no agent in it: no parent, but still a hire.
    expect(movements([], [agent("temp", { depth: 1 })], "ws")).toEqual([{ kind: "arrive", id: "temp" }]);
  });

  it("does not walk in someone who was simply started in a terminal", () => {
    expect(movements([], [agent("boss")], "ws")).toEqual([]);
  });

  it("walks out whoever has just ended", () => {
    const before = [agent("kid", { depth: 1 })];
    expect(movements(before, [agent("kid", { depth: 1, status: "dead" })], "ws")).toEqual([{ kind: "leave", id: "kid" }]);
  });

  it("walks out whoever has vanished from the list altogether", () => {
    expect(movements([agent("kid")], [], "ws")).toEqual([{ kind: "leave", id: "kid" }]);
  });

  it("does not walk anyone anywhere on the first load", () => {
    // Opening the app on a full office must not march six people in.
    expect(movements(null, [agent("kid", { parent_id: "boss", depth: 1 })], "ws")).toEqual([]);
  });

  it("does not see off someone who arrived already dead", () => {
    expect(movements([], [agent("ghost", { depth: 1, status: "dead" })], "ws")).toEqual([]);
  });

  it("minds its own workspace", () => {
    const after = [agent("kid", { depth: 1, workspace_id: "other" })];
    expect(movements([], after, "ws")).toEqual([]);
  });

  it("reports departures before arrivals, so a freed pod can be walked to", () => {
    const before = [agent("old", { depth: 1 })];
    const after = [agent("old", { depth: 1, status: "dead" }), agent("new", { depth: 1 })];
    expect(movements(before, after, "ws").map((m) => m.kind)).toEqual(["leave", "arrive"]);
  });
});

describe("legsTo", () => {
  it("walks the design's route to the design's sixth pod", () => {
    expect(legsTo(5)).toEqual([
      { x: 150, y: 400, seconds: 0.9 },
      { x: 1040, y: 400, seconds: 1.9 },
      { x: 1040, y: 560, seconds: 0.8 },
    ]);
  });

  it("turns up into a pod in the top row", () => {
    const legs = legsTo(0);
    expect(legs[1]).toMatchObject({ x: 420, y: 400 });
    expect(legs[2]).toMatchObject({ x: 420, y: 210 });
  });

  it("uses the corridor beside a lower row rather than walking through desks", () => {
    const legs = legsTo(8);
    expect(legs[0]).toMatchObject({ x: HR_DOOR.x, y: 1100 });
    expect(legs[2]).toMatchObject({ x: 1040, y: 910 });
  });

  it("takes longer over a longer walk, at the design's pace", () => {
    expect(legsTo(3)[1].seconds).toBeLessThan(legsTo(5)[1].seconds);
    expect(legsTo(8)[0].seconds).toBeGreaterThan(legsTo(5)[0].seconds);
  });
});

describe("routeOf", () => {
  it("starts an arrival at HR's door", () => {
    const route = routeOf({ kind: "arrive", pod: 5 });
    expect(route.from).toEqual(HR_DOOR);
    expect(route.legs).toEqual(legsTo(5));
  });

  it("walks a leaver the same way back, at the same pace", () => {
    const route = routeOf({ kind: "leave", pod: 5 });
    expect(route.from).toEqual({ x: 1040, y: 560 });
    expect(route.legs).toEqual([
      { x: 1040, y: 400, seconds: 0.8 },
      { x: 150, y: 400, seconds: 1.9 },
      { x: HR_DOOR.x, y: HR_DOOR.y, seconds: 0.9 },
    ]);
  });
});

describe("walkDuration", () => {
  it("is the pause at HR's door plus every leg", () => {
    expect(walkDuration(legsTo(5))).toBeCloseTo(0.6 + 0.9 + 1.9 + 0.8);
  });
});

describe("enqueue", () => {
  const walk = (id: string, kind: Walk["kind"] = "arrive"): Walk => ({ kind, id, pod: 0 });

  it("queues walks in the order they were asked for", () => {
    expect(enqueue([walk("a")], [walk("b"), walk("c")]).map((w) => w.id)).toEqual(["a", "b", "c"]);
  });

  it("does not queue the same walk twice", () => {
    expect(enqueue([walk("a")], [walk("a")]).map((w) => w.id)).toEqual(["a"]);
  });

  it("cancels an arrival that has not started when the agent has already gone", () => {
    const queue = enqueue([walk("first"), walk("b")], [walk("b", "leave")]);
    expect(queue.map((w) => `${w.kind}:${w.id}`)).toEqual(["arrive:first"]);
  });

  it("lets whoever is already walking finish, then sees them out", () => {
    const queue = enqueue([walk("a")], [walk("a", "leave")]);
    expect(queue.map((w) => `${w.kind}:${w.id}`)).toEqual(["arrive:a", "leave:a"]);
  });

  it("queues nothing for someone who has asked for less motion", () => {
    expect(enqueue([], [walk("a")], { still: true })).toEqual([]);
  });
});
