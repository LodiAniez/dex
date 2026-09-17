/**
 * What an agent with nothing to do gets up to. An agent whose task is done sits
 * idle until someone gives it another, and a floor of people sitting still is a
 * dull thing to watch; so they fool around, and go back to their desk like a
 * normal person the moment there is work. The rules only: who is loafing, what
 * they pick, and where it happens. `IdleActor.tsx` does the walking and the
 * drawing.
 */

import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { between, corridorOf, deskOf, type Leg, type Spot } from "./walks";

export type AnticKind = "coffee" | "nap" | "kart" | "rope" | "roll" | "sing" | "tumble";

/** Each antic: whether it takes them away from their desk, and how long it lasts when it stays in one place. */
export const ANTICS: Record<AnticKind, { away: boolean; seconds: number }> = {
  coffee: { away: true, seconds: 9 },
  nap: { away: false, seconds: 14 },
  kart: { away: true, seconds: 0 },
  rope: { away: true, seconds: 8 },
  roll: { away: true, seconds: 0 },
  sing: { away: false, seconds: 9 },
  tumble: { away: true, seconds: 0 },
};

const KINDS = Object.keys(ANTICS) as AnticKind[];

/**
 * Whether an agent has nothing to do. Idle, and with Claude Code started: a
 * hire shows idle for its first seconds too, and is about to be handed its task.
 */
export function isLoafing(agent: { status: AgentStatus; started: boolean }): boolean {
  return agent.status === "idle" && agent.started;
}

function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  h ^= h >>> 15;
  h = Math.imul(h, 0x2c1b3c6d);
  h ^= h >>> 12;
  return h >>> 0;
}

/**
 * What they do on the `round`-th go of this idle spell. Random to look at, but
 * worked out from who they are and when they went idle, so every window shows
 * the same thing; and never the same thing twice running.
 */
export function pickAntic(agentId: string, idleSince: number, round: number): AnticKind {
  let index = hash(`${agentId}:${idleSince}`) % KINDS.length;
  for (let r = 1; r <= round; r += 1) {
    // A step of 1..6 around a ring of 7 can never land where it started.
    index = (index + 1 + (hash(`${agentId}:${idleSince}:${r}`) % (KINDS.length - 1))) % KINDS.length;
  }
  return KINDS[index];
}

/** The break room: where its coffee drinkers stand, along the counter. */
const BREAK_ROOM = { y: 596, firstX: 56, step: 50, places: 4 } as const;
/** The main corridor, which the break room opens off. */
const MAIN_CORRIDOR = 400;
/** The stretch of a corridor antics may use: clear of HR's column and of the loudspeaker. */
const CORRIDOR = { left: 300, right: 1100 - 48 - 1 } as const;
/** How far a roll or a tumble carries them, and the least worth setting off for. */
const CARRY = { roll: 200, tumble: 260, least: 100 } as const;
/** Units a second: a kart is quick, a roll is not, a tumble is in between. */
const SPEED = { kart: 900, roll: 110, tumble: 170 } as const;
/** Where a kart turns round at each end of the corridor. */
const LAP = { left: 340, right: 1040, laps: 2 } as const;

const timed = (distance: number, speed: number) => Math.round((Math.abs(distance) / speed) * 1000) / 1000;

export interface AnticPlan {
  kind: AnticKind;
  /** Where it happens, or null for something done at the desk. */
  to: Spot | null;
  /** Where the antic itself takes them once there; empty for one done on the spot. */
  act: Leg[];
  /** How long it lasts. */
  seconds: number;
}

/**
 * Where an antic happens and where it takes them. Coffee is in the break room;
 * everything else away from the desk is in the corridor outside their own
 * cubicle, never inside anyone's, and clear of HR and the loudspeaker.
 */
export function planAntic(kind: AnticKind, pod: number, seed: number): AnticPlan {
  if (!ANTICS[kind].away) return { kind, to: null, act: [], seconds: ANTICS[kind].seconds };
  if (kind === "coffee") {
    const x = BREAK_ROOM.firstX + (seed % BREAK_ROOM.places) * BREAK_ROOM.step;
    return { kind, to: { x, y: BREAK_ROOM.y, corridor: MAIN_CORRIDOR }, act: [], seconds: ANTICS.coffee.seconds };
  }
  const y = corridorOf(pod);
  const to: Spot = { x: deskOf(pod).x, y, corridor: y };
  const legs = (xs: number[], speed: number): Leg[] => {
    const stops = [to.x, ...xs];
    return xs.map((x, i) => ({ x, y, seconds: timed(x - stops[i], speed) })).filter((leg) => leg.seconds > 0);
  };
  let act: Leg[] = [];
  if (kind === "kart") {
    const lap = Array.from({ length: LAP.laps }, () => [LAP.left, LAP.right]).flat();
    act = legs([...lap, to.x], SPEED.kart);
  } else if (kind === "roll" || kind === "tumble") {
    const clamp = (x: number) => Math.min(CORRIDOR.right, Math.max(CORRIDOR.left, x));
    const wanted = seed % 2 === 0 ? 1 : -1;
    // The way they fancied, unless a wall is in it.
    const direction = Math.abs(clamp(to.x + wanted * CARRY[kind]) - to.x) >= CARRY.least ? wanted : -wanted;
    act = legs([clamp(to.x + direction * CARRY[kind]), to.x], SPEED[kind]);
  }
  const seconds = act.length > 0 ? Math.round(act.reduce((sum, leg) => sum + leg.seconds, 0) * 1000) / 1000 : ANTICS[kind].seconds;
  return { kind, to, act, seconds };
}

/** Back to their own desk from wherever they are, at a walk, by the corridor. */
export function wayHome(from: Spot, pod: number): Leg[] {
  const desk = deskOf(pod);
  // `between` goes by the corridor, which from their own desk would be a walk round the block.
  if (from.x === desk.x && from.y === desk.y) return [];
  return between(from, desk);
}
