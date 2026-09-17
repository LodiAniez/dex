import { describe, expect, it } from "vitest";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { EXIT_DOOR, HR_DOOR, TALK_SECONDS, WAVE_SECONDS, afterWalk, awayFromDesk, deliveries, enqueue, legsTo, movements, nextLegs, routeOf, walkDuration, type Walk } from "./walks";

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
    }
  });
});

describe("routeOf", () => {
  it("starts an arrival at HR's door", () => {
    const route = routeOf({ kind: "arrive", pod: 5 });
    expect(route.from).toEqual(HR_DOOR);
    expect(route.legs).toEqual(legsTo(5));
  });

  it("walks a leaver out of the door, not back into HR, which only hires", () => {
    const route = routeOf({ kind: "leave", pod: 5 });
    expect(route.from).toEqual({ x: 1040, y: 560 });
    expect(route.legs.map(({ x, y }) => [x, y])).toEqual([
      [1040, 400], // out of their pod, up to the corridor
      [EXIT_DOOR.x, EXIT_DOOR.y], // and along it to the door at its end
    ]);
    expect(route.legs.at(-1)).not.toMatchObject({ x: HR_DOOR.x, y: HR_DOOR.y });
  });

  it("puts the door on the left wall at the end of the main corridor, clear of HR and the break room", () => {
    // HR's room ends at y = 342 and the break room starts at y = 560.
    expect(EXIT_DOOR.y).toBe(400);
    expect(EXIT_DOOR.x).toBeLessThan(40);
    expect(EXIT_DOOR.y).toBeGreaterThan(342);
    expect(EXIT_DOOR.y + 60).toBeLessThan(560);
  });

  it("brings someone from a lower row up the side aisle to the door, never through desks", () => {
    const stops = routeOf({ kind: "leave", pod: 8 }).legs.map(({ x, y }) => [x, y]);
    expect(stops).toEqual([
      [1040, 1100], // their own corridor
      [HR_DOOR.x, 1100], // along it to the aisle
      [HR_DOOR.x, 400], // up the aisle to the main corridor
      [EXIT_DOOR.x, EXIT_DOOR.y],
    ]);
  });

  it("takes them the same time per step as any other walk", () => {
    const legs = routeOf({ kind: "leave", pod: 5 }).legs;
    expect(legs[0].seconds).toBe(0.9);
    expect(legs[1].seconds).toBeGreaterThan(1.9); // further than HR's column: all the way to the wall
  });

  it("gives them a moment at the door to wave", () => {
    expect(WAVE_SECONDS).toBeGreaterThanOrEqual(1);
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

  it("sends the sender on one round of everyone they wrote to, in the order they wrote", () => {
    const walks = deliveries([message(7, "lead", "a"), message(8, "lead", "b")], 6, seats);
    expect(walks).toEqual([{ kind: "deliver", id: "lead", pod: 0, stops: [1, 4], keys: ["m7", "m8"], key: "m7" }]);
  });

  it("gives each sender a round of their own", () => {
    const walks = deliveries([message(7, "lead", "a"), message(8, "a", "lead"), message(9, "lead", "b")], 0, seats);
    expect(walks.map((w) => [w.id, w.stops])).toEqual([["lead", [1, 4]], ["a", [0]]]);
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

describe("nextLegs: a round, one decision at a time", () => {
  const round: Walk = { kind: "deliver", id: "lead", pod: 0, stops: [2, 1], keys: ["m1", "m2"], key: "m1" };
  const stops = (legs: { x: number; y: number }[]) => legs.map(({ x, y }) => [x, y]);

  it("sets off from their own desk to the first colleague, along the corridor, and stops to talk", () => {
    const next = nextLegs(round, 0, "home");
    expect(next?.arrives).toBe(2);
    expect(stops(next?.legs ?? [])).toEqual([
      [420, 400], // out of their own pod
      [990, 400], // along the corridor
      [990, 240], // in beside the colleague's chair, not on top of them
    ]);
  });

  it("goes straight from one colleague to the next, without going home in between", () => {
    const next = nextLegs(round, 1, 2);
    expect(next?.arrives).toBe(1);
    expect(stops(next?.legs ?? [])).toEqual([
      [990, 400],
      [680, 400],
      [680, 240],
    ]);
  });

  it("goes home once everyone has been told", () => {
    const next = nextLegs(round, 2, 1);
    expect(next?.arrives).toBe("home");
    expect(stops(next?.legs ?? [])).toEqual([
      [680, 400],
      [420, 400],
      [420, 210],
    ]);
  });

  it("is finished when they are home with nobody left to tell", () => {
    expect(nextLegs(round, 2, "home")).toBeNull();
  });

  it("goes back out from home if another message turned up on the way back", () => {
    const longer = { ...round, stops: [2, 1, 4] };
    expect(nextLegs(longer, 2, "home")?.arrives).toBe(4);
  });

  it("uses the side aisle between corridors, never a row of desks", () => {
    const far: Walk = { kind: "deliver", id: "a", pod: 1, stops: [7], key: "m1" };
    expect(stops(nextLegs(far, 0, "home")?.legs ?? [])).toEqual([
      [730, 400],
      [HR_DOOR.x, 400],
      [HR_DOOR.x, 1100],
      [680, 1100],
      [680, 940],
    ]);
  });

  it("never walks as far as the loudspeaker", () => {
    for (let pod = 0; pod < 12; pod += 1) {
      const visit = nextLegs({ pod: 0, stops: [pod] }, 0, "home");
      for (const leg of visit?.legs ?? []) expect(leg.x + 48, `visit to ${pod}`).toBeLessThan(1100);
    }
  });

  it("takes a moment over the conversation", () => {
    expect(TALK_SECONDS).toBeGreaterThanOrEqual(2);
  });
});

describe("enqueue, with deliveries", () => {
  const round = (id: string, stops: number[], keys: string[]): Walk => ({ kind: "deliver", id, pod: 0, stops, keys, key: keys[0] });

  it("adds a later message to the round its sender is already on, even mid-walk", () => {
    // A lead's three message_send calls land seconds apart; the first has set
    // him walking by the time the second arrives.
    const queue = enqueue([round("lead", [1], ["m7"])], [round("lead", [4], ["m8"])]);
    expect(queue).toHaveLength(1);
    expect(queue[0]).toMatchObject({ stops: [1, 4], keys: ["m7", "m8"], key: "m7" });
  });

  it("does not deliver the same message twice", () => {
    const queue = enqueue([round("lead", [1], ["m7"])], [round("lead", [1, 4], ["m7", "m8"])]);
    expect(queue[0]).toMatchObject({ stops: [1, 4], keys: ["m7", "m8"] });
  });

  it("keeps different senders' rounds apart", () => {
    const queue = enqueue([round("lead", [1], ["m7"])], [round("a", [0], ["m8"])]);
    expect(queue.map((w) => w.id)).toEqual(["lead", "a"]);
  });

  it("drops the round of a sender who has left before setting off", () => {
    const queue = enqueue([round("x", [1], ["m1"]), round("lead", [1, 2], ["m7", "m8"])], [{ kind: "leave", id: "lead", pod: 0 }]);
    expect(queue.map((w) => `${w.kind}:${w.id}`)).toEqual(["deliver:x", "leave:lead"]);
  });
});

describe("awayFromDesk", () => {
  it("is whoever is out delivering right now, and nobody who is only waiting to", () => {
    const queue: Walk[] = [
      { kind: "deliver", id: "lead", pod: 0, stops: [1], key: "m7" },
      { kind: "deliver", id: "a", pod: 1, stops: [0], key: "m8" },
    ];
    expect(awayFromDesk(queue)).toBe("lead");
    expect(awayFromDesk([{ kind: "arrive", id: "new", pod: 3 }])).toBeNull();
    expect(awayFromDesk([])).toBeNull();
  });
});

describe("afterWalk", () => {
  const round = (stops: number[], keys: string[]): Walk => ({ kind: "deliver", id: "lead", pod: 0, stops, keys, key: keys[0] });
  const next: Walk = { kind: "arrive", id: "new", pod: 3 };

  it("takes whoever has finished off the head of the queue", () => {
    expect(afterWalk([round([1, 2], ["m1", "m2"]), next], 2)).toEqual([next]);
    expect(afterWalk([next], undefined)).toEqual([]);
  });

  it("sends them out again for a message that arrived as they sat down", () => {
    // The walker decided it was done a moment before the message was added
    // to its round. Dropping the round now would lose the message from view.
    const queue = afterWalk([round([1, 2, 4], ["m1", "m2", "m3"]), next], 2);
    expect(queue).toEqual([{ kind: "deliver", id: "lead", pod: 0, stops: [4], keys: ["m3"], key: "m3" }, next]);
  });

  it("copes with an empty queue", () => {
    expect(afterWalk([], 0)).toEqual([]);
  });
});
