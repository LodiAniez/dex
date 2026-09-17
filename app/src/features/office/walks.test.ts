import { describe, expect, it } from "vitest";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { HR_DOOR, VISIT_SECONDS, awayFromDesk, deliveries, enqueue, legsTo, movements, routeOf, walkDuration, type Walk } from "./walks";

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

describe("the corridor", () => {
  it("is never walked as far as the loudspeaker beside the water cooler", () => {
    // The speaker stands in the corridor from x = 1100; a walker is 48 wide.
    for (let pod = 0; pod < 12; pod += 1) {
      for (const leg of legsTo(pod)) expect(leg.x + 48, `pod ${pod}`).toBeLessThan(1100);
      for (const leg of routeOf({ kind: "deliver", pod: 0, to: pod }).legs) expect(leg.x + 48, `visit to ${pod}`).toBeLessThan(1100);
    }
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

describe("deliveries", () => {
  const seats = new Map([["lead", 0], ["a", 1], ["b", 4]]);
  const message = (seq: number, from: string | null, to: string | null) => ({ seq, kind: "message", agent_id: from, target_agent_id: to });

  it("walks the sender to each recipient's desk, a trip per message", () => {
    const walks = deliveries([message(7, "lead", "a"), message(8, "lead", "b")], 6, seats);
    expect(walks).toEqual([
      { kind: "deliver", id: "lead", pod: 0, to: 1, key: "m7" },
      { kind: "deliver", id: "lead", pod: 0, to: 4, key: "m8" },
    ]);
  });

  it("walks nobody for a message that was already there when the office opened", () => {
    expect(deliveries([message(5, "lead", "a")], 6, seats)).toEqual([]);
  });

  it("walks nobody for a memo from the owner, who has no desk to walk from", () => {
    expect(deliveries([message(7, null, "a")], 0, seats)).toEqual([]);
  });

  it("walks nobody to or from someone who is not at a desk here, or to themselves", () => {
    expect(deliveries([message(7, "lead", "gone"), message(8, "gone", "a"), message(9, "a", "a")], 0, seats)).toEqual([]);
  });

  it("ignores everything that is not a message", () => {
    expect(deliveries([{ seq: 9, kind: "note", agent_id: "lead", target_agent_id: null }], 0, seats)).toEqual([]);
  });
});

describe("a delivery's route", () => {
  it("goes out along the corridor, waits at the desk, and comes back the same way", () => {
    const route = routeOf({ kind: "deliver", pod: 0, to: 2 });
    expect(route.from).toEqual({ x: 420, y: 210 });
    const stops = route.legs.map(({ x, y }) => [x, y]);
    expect(stops).toEqual([
      [420, 400], // out of their own pod
      [990, 400], // along the corridor
      [990, 240], // in beside the recipient, not on top of them
      [990, 240], // the message is handed over
      [990, 400],
      [420, 400],
      [420, 210], // and home
    ]);
    expect(route.legs[3].seconds).toBe(VISIT_SECONDS);
  });

  it("uses the side aisle between corridors, never a row of desks", () => {
    const stops = routeOf({ kind: "deliver", pod: 1, to: 7 }).legs.map(({ x, y }) => [x, y]);
    expect(stops.slice(0, 5)).toEqual([
      [730, 400],
      [HR_DOOR.x, 400],
      [HR_DOOR.x, 1100],
      [680, 1100],
      [680, 940],
    ]);
  });

  it("takes time in proportion to the distance", () => {
    const near = walkDuration(routeOf({ kind: "deliver", pod: 0, to: 1 }).legs);
    const far = walkDuration(routeOf({ kind: "deliver", pod: 0, to: 2 }).legs);
    expect(far).toBeGreaterThan(near);
  });
});

describe("enqueue, with deliveries", () => {
  const deliver = (id: string, to: number, key: string): Walk => ({ kind: "deliver", id, pod: 0, to, key });

  it("queues several trips by one sender, which share an id and a kind", () => {
    const queue = enqueue([], [deliver("lead", 1, "m7"), deliver("lead", 2, "m8")]);
    expect(queue.map((w) => w.key)).toEqual(["m7", "m8"]);
  });

  it("does not queue the same message twice", () => {
    expect(enqueue([deliver("lead", 1, "m7")], [deliver("lead", 1, "m7")])).toHaveLength(1);
  });

  it("drops the trips of a sender who has left before setting off", () => {
    const queue = enqueue([deliver("x", 1, "m1"), deliver("lead", 1, "m7"), deliver("lead", 2, "m8")], [{ kind: "leave", id: "lead", pod: 0 }]);
    expect(queue.map((w) => `${w.kind}:${w.id}`)).toEqual(["deliver:x", "leave:lead"]);
  });
});

describe("awayFromDesk", () => {
  it("is whoever is out delivering right now, and nobody who is only waiting to", () => {
    const queue: Walk[] = [
      { kind: "deliver", id: "lead", pod: 0, to: 1, key: "m7" },
      { kind: "deliver", id: "a", pod: 1, to: 0, key: "m8" },
    ];
    expect(awayFromDesk(queue)).toBe("lead");
    expect(awayFromDesk([{ kind: "arrive", id: "new", pod: 3 }])).toBeNull();
    expect(awayFromDesk([])).toBeNull();
  });
});
