import { describe, expect, it } from "vitest";
import { ANTICS, isLoafing, pickAntic, planAntic, wayHome, type AnticKind } from "./antics";
import { POD, podOrigin } from "./mapGeometry";
import { deskOf } from "./walks";

const KINDS: AnticKind[] = ["coffee", "nap", "kart", "rope", "roll", "sing", "tumble"];

describe("the antics", () => {
  it("are the seven that were asked for", () => {
    expect(Object.keys(ANTICS).sort()).toEqual([...KINDS].sort());
  });

  it("happen at the desk for a nap and a song, and away from it for the rest", () => {
    expect(KINDS.filter((kind) => !ANTICS[kind].away).sort()).toEqual(["nap", "sing"]);
  });
});

describe("isLoafing", () => {
  it("is an agent with nothing to do whose Claude Code has started", () => {
    expect(isLoafing({ status: "idle", started: true })).toBe(true);
  });

  it("is not a hire who has not started: they are about to be given their task", () => {
    expect(isLoafing({ status: "idle", started: false })).toBe(false);
  });

  it("is nobody who is working, waiting on the owner, stopped or gone quiet", () => {
    for (const status of ["running", "waiting", "error", "unknown", "dead"] as const) {
      expect(isLoafing({ status, started: true }), status).toBe(false);
    }
  });
});

describe("pickAntic", () => {
  it("is the same for the same agent, idle spell and round, so every window agrees", () => {
    expect(pickAntic("agent-1", 1000, 3)).toBe(pickAntic("agent-1", 1000, 3));
  });

  it("gets round to all seven", () => {
    const seen = new Set<AnticKind>();
    for (let round = 0; round < 200; round += 1) seen.add(pickAntic("agent-1", 1000, round));
    expect([...seen].sort()).toEqual([...KINDS].sort());
  });

  it("never does the same thing twice running", () => {
    for (const id of ["a", "b", "agent-7f3a"]) {
      for (let round = 1; round < 300; round += 1) {
        expect(pickAntic(id, 5, round), `${id} round ${round}`).not.toBe(pickAntic(id, 5, round - 1));
      }
    }
  });

  it("differs between agents, so the office is not a chorus line", () => {
    const first = new Set(["a", "b", "c", "d", "e", "f", "g", "h"].map((id) => pickAntic(id, 1000, 0)));
    expect(first.size).toBeGreaterThan(2);
  });
});

describe("planAntic", () => {
  const insideAPod = (x: number, y: number) => {
    for (let pod = 0; pod < 12; pod += 1) {
      const o = podOrigin(pod);
      // A figure is 48 wide and 60 tall, standing at its top-left.
      if (x + 48 > o.x && x < o.x + POD.width && y + 60 > o.y + 30 && y < o.y + POD.height) return pod;
    }
    return null;
  };

  it("keeps a nap and a song at the desk, with nowhere to walk", () => {
    for (const kind of ["nap", "sing"] as const) {
      const plan = planAntic(kind, 4, 9);
      expect(plan.to).toBeNull();
      expect(plan.act).toEqual([]);
      expect(plan.seconds).toBeGreaterThan(5);
    }
  });

  it("takes coffee in the break room", () => {
    // The break room spans x 36..266, y 560..664.
    for (const seed of [0, 1, 2, 3, 4, 5]) {
      const { to } = planAntic("coffee", 2, seed);
      expect(to).not.toBeNull();
      expect(to!.x).toBeGreaterThanOrEqual(36);
      expect(to!.x + 48).toBeLessThanOrEqual(266);
      expect(to!.y).toBeGreaterThanOrEqual(540);
      expect(to!.y).toBeLessThanOrEqual(664);
    }
  });

  it("spreads the coffee drinkers out, so two are not one figure", () => {
    const spots = new Set([0, 1, 2, 3, 4, 5].map((seed) => planAntic("coffee", 2, seed).to!.x));
    expect(spots.size).toBeGreaterThan(1);
  });

  it("does everything else in the corridor outside their own cubicle, never inside anyone's", () => {
    for (const kind of ["kart", "rope", "roll", "tumble"] as const) {
      for (let pod = 0; pod < 12; pod += 1) {
        for (const seed of [0, 1, 2, 3]) {
          const plan = planAntic(kind, pod, seed);
          const stops = [plan.to!, ...plan.act];
          for (const stop of stops) {
            expect(insideAPod(stop.x, stop.y), `${kind} from pod ${pod} at ${stop.x},${stop.y}`).toBeNull();
            // Clear of HR's column on the left and the loudspeaker on the right.
            expect(stop.x).toBeGreaterThanOrEqual(300);
            expect(stop.x + 48).toBeLessThan(1100);
          }
        }
      }
    }
  });

  it("drives the kart up and down the corridor, fast, and brings it back", () => {
    const plan = planAntic("kart", 1, 0);
    expect(plan.act.length).toBeGreaterThanOrEqual(4);
    expect(new Set(plan.act.map((leg) => leg.y)).size).toBe(1);
    const span = Math.max(...plan.act.map((leg) => leg.x)) - Math.min(...plan.act.map((leg) => leg.x));
    expect(span).toBeGreaterThan(500);
    // Faster than anyone walks: the corridor's 890 units take a walker 1.9 s.
    const first = plan.act[0];
    expect(Math.abs(first.x - plan.to!.x) / first.seconds).toBeGreaterThan(890 / 1.9);
    expect(plan.act.at(-1)).toMatchObject({ x: plan.to!.x, y: plan.to!.y });
  });

  it("rolls and tumbles away in a direction of their own, and back to where they started", () => {
    for (const kind of ["roll", "tumble"] as const) {
      const directions = new Set<number>();
      for (const seed of [0, 1, 2, 3, 4, 5, 6, 7]) {
        const plan = planAntic(kind, 4, seed);
        expect(plan.act.at(-1)).toMatchObject({ x: plan.to!.x, y: plan.to!.y });
        directions.add(Math.sign(plan.act[0].x - plan.to!.x));
      }
      expect(directions, kind).toEqual(new Set([-1, 1]));
    }
  });

  it("skips rope on the spot", () => {
    const plan = planAntic("rope", 4, 0);
    expect(plan.act).toEqual([]);
    expect(plan.seconds).toBeGreaterThan(4);
  });
});

describe("wayHome", () => {
  it("walks back to their own desk from wherever they were, by the corridor", () => {
    const from = planAntic("coffee", 5, 0).to!;
    const legs = wayHome(from, 5);
    expect(legs.at(-1)).toMatchObject({ x: deskOf(5).x, y: deskOf(5).y });
    expect(legs.every((leg) => leg.seconds > 0)).toBe(true);
    // At a walk: no leg is covered faster than the corridor pace.
    let at = { x: from.x, y: from.y };
    for (const leg of legs) {
      const distance = Math.abs(leg.x - at.x) + Math.abs(leg.y - at.y);
      expect(distance / leg.seconds).toBeLessThanOrEqual(890 / 1.9 + 1);
      at = leg;
    }
  });

  it("has nowhere to go for someone already home", () => {
    expect(wayHome(deskOf(3), 3)).toEqual([]);
  });
});
