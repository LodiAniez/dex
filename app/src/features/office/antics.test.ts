import { describe, expect, it } from "vitest";
import { ANTICS, canBreakOffAt, isLoafing, pickAntic, planAntic, seedOf, wayHome, type AnticKind } from "./antics";
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

  it("never picks what they were told they have just done, whatever spell or window the count is from", () => {
    for (const not of KINDS) {
      for (let round = 0; round < 60; round += 1) {
        for (const since of [5, 6, 7]) expect(pickAntic("agent-1", since, round, not), `${not} ${since} ${round}`).not.toBe(not);
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

  it("stands them anywhere in the room, not on one mark", () => {
    const spots = Array.from({ length: 40 }, (_, seed) => planAntic("coffee", 2, seed).to!);
    expect(new Set(spots.map((spot) => spot.x)).size).toBeGreaterThan(3);
    expect(new Set(spots.map((spot) => spot.y)).size).toBeGreaterThan(1);
  });

  it("never stands one drinker on another, however many are in there at once", () => {
    const taken: { x: number; y: number }[] = [];
    for (let drinker = 0; drinker < 10; drinker += 1) {
      const { to } = planAntic("coffee", drinker, 7, { taken });
      for (const other of taken) expect(Math.hypot(to!.x - other.x, to!.y - other.y), `drinker ${drinker}`).toBeGreaterThan(20);
      expect(to!.x).toBeGreaterThanOrEqual(36);
      expect(to!.x + 48).toBeLessThanOrEqual(266);
      taken.push(to!);
    }
  });

  it("gives two drinkers elbow room while the room has it", () => {
    for (let seed = 0; seed < 24; seed += 1) {
      const first = planAntic("coffee", 0, seed).to!;
      const second = planAntic("coffee", 1, seed, { taken: [first] }).to!;
      expect(Math.hypot(first.x - second.x, first.y - second.y), `seed ${seed}`).toBeGreaterThan(40);
    }
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

  it("sets off either way along the corridor, and turns round somewhere of its own", () => {
    const plans = Array.from({ length: 40 }, (_, seed) => planAntic("kart", 1, seed));
    expect(new Set(plans.map((plan) => Math.sign(plan.act[0].x - plan.to!.x)))).toEqual(new Set([-1, 1]));
    const turns = new Set(plans.flatMap((plan) => plan.act.map((leg) => leg.x)));
    expect(turns.size).toBeGreaterThan(6);
  });

  it("takes some laps round the block, both ways, when the floor has room below", () => {
    const plans = Array.from({ length: 40 }, (_, seed) => planAntic("kart", 1, seed, { mapHeight: 832 }));
    const circuits = plans.filter((plan) => new Set(plan.act.map((leg) => leg.y)).size > 1);
    expect(circuits.length).toBeGreaterThan(0);
    expect(circuits.length).toBeLessThan(plans.length);
    // Round the block one way, and the other.
    expect(new Set(circuits.map((plan) => Math.sign(plan.act[0].x - plan.act.at(-2)!.x))).size).toBe(2);
    for (const plan of circuits) {
      expect(plan.act.at(-1)).toMatchObject({ x: plan.to!.x, y: plan.to!.y });
      for (const [i, leg] of plan.act.entries()) {
        const from = i === 0 ? plan.to! : plan.act[i - 1];
        // Along or down, never across a cubicle on the diagonal.
        expect(leg.x === from.x || leg.y === from.y, `leg ${i}`).toBe(true);
        expect(insideAPod(leg.x, leg.y)).toBeNull();
        expect(leg.y + 66).toBeLessThanOrEqual(832 - 14);
        // Down the gaps between the columns of cubicles, and nowhere else.
        if (leg.x === from.x) for (let pod = 0; pod < 12; pod += 1) expect(leg.x + 48 <= podOrigin(pod).x || leg.x >= podOrigin(pod).x + POD.width).toBe(true);
      }
    }
  });

  it("stays in its corridor on a floor with nothing below it", () => {
    // A third row's corridor is the map's last: there is no block to go round.
    for (let seed = 0; seed < 40; seed += 1) {
      const plan = planAntic("kart", 7, seed, { mapHeight: 1182 });
      expect(new Set(plan.act.map((leg) => leg.y)).size, `seed ${seed}`).toBe(1);
    }
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

describe("canBreakOffAt", () => {
  it("lets a kart driver stop anywhere on their own corridor, but not at the bottom of the block: the way home starts from a corridor", () => {
    const plans = Array.from({ length: 40 }, (_, seed) => planAntic("kart", 1, seed, { mapHeight: 832 }));
    const circuit = plans.find((plan) => new Set(plan.act.map((leg) => leg.y)).size > 1)!;
    for (const leg of circuit.act) expect(canBreakOffAt(circuit, leg), `${leg.x},${leg.y}`).toBe(leg.y === circuit.to!.corridor);
    expect(circuit.act.some((leg) => !canBreakOffAt(circuit, leg))).toBe(true);
  });

  it("lets anyone rolling, tumbling or lapping the corridor stop at any turn", () => {
    for (const kind of ["roll", "tumble", "kart"] as const) {
      const plan = planAntic(kind, 4, 1);
      for (const leg of plan.act) expect(canBreakOffAt(plan, leg)).toBe(true);
    }
  });
});

describe("seedOf", () => {
  it("is the same in every window, and different from one go to the next", () => {
    expect(seedOf("agent-1", 1000, 2)).toBe(seedOf("agent-1", 1000, 2));
    expect(new Set([0, 1, 2, 3, 4, 5].map((round) => seedOf("agent-1", 1000, round))).size).toBeGreaterThan(4);
    expect(Number.isInteger(seedOf("agent-1", 1000, 2))).toBe(true);
    expect(seedOf("agent-1", 1000, 2)).toBeGreaterThanOrEqual(0);
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
