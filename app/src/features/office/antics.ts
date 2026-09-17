/**
 * What an agent with nothing to do gets up to. An agent whose task is done sits
 * idle until someone gives it another, and a floor of people sitting still is a
 * dull thing to watch; so they fool around, and go back to their desk like a
 * normal person the moment there is work. The rules only: who is loafing, what
 * they pick, and where it happens. `IdleActor.tsx` does the walking and the
 * drawing.
 */

import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { POD, WALL_INSET, podOrigin } from "./mapGeometry";
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

/**
 * What they do with the go: which mark in the break room, which way the kart
 * sets off. Worked out like the antic itself, so every window agrees.
 */
export function seedOf(agentId: string, idleSince: number, round: number): number {
  return hash(`${agentId}:${idleSince}:${round}:how`);
}

/** The break room: a grid of places to stand, far enough apart that nobody is stood on anybody. */
const BREAK_ROOM = { xs: [44, 77, 110, 143, 176, 209], ys: [578, 604], apart: [44, 24] } as const;
/** The main corridor, which the break room opens off. */
const MAIN_CORRIDOR = 400;
/** The stretch of a corridor antics may use: clear of HR's column and of the loudspeaker. */
const CORRIDOR = { left: 300, right: 1100 - 48 - 1 } as const;
/** How far a roll or a tumble carries them, and the least worth setting off for. */
const CARRY = { roll: 200, tumble: 260, least: 100 } as const;
/** Units a second: a kart is quick, a roll is not, a tumble is in between. */
const SPEED = { kart: 900, roll: 110, tumble: 170 } as const;
/** Where a kart may turn round at each end of the corridor, and how many times it goes round. */
const LAP = { left: 320, right: 1040, give: 20, laps: 2 } as const;
/** A figure is this wide; a kart on its wheels stands this tall. */
const FIGURE_WIDTH = 48;
const KART_HEIGHT = 66;
/** The floor below a corridor's cubicles is this far down from it. */
const BLOCK_DEPTH = 340;

/** What a plan may need to know about the floor as it is right now. */
export interface FloorNow {
  /** Where people are already standing with a coffee. */
  taken?: readonly { x: number; y: number }[];
  /** The map's height. Without it nobody leaves their corridor. */
  mapHeight?: number;
}

/** A place in the break room nobody is standing, starting the search somewhere of their own. */
function coffeeSpot(seed: number, taken: readonly { x: number; y: number }[]): { x: number; y: number } {
  const places = BREAK_ROOM.ys.flatMap((y) => BREAK_ROOM.xs.map((x) => ({ x, y })));
  // With elbow room if there is any; shoulder to shoulder once the room fills up.
  for (const apart of BREAK_ROOM.apart) {
    // A step of 5 round 12 places visits every one of them.
    for (let i = 0; i < places.length; i += 1) {
      const place = places[(seed + i * 5) % places.length];
      if (taken.every((other) => Math.hypot(place.x - other.x, place.y - other.y) > apart)) return place;
    }
  }
  return places[seed % places.length];
}

/** The corners of a lap round the block below `corridor`, down one gap between columns of cubicles and up the next; null if the floor ends first. */
function blockBelow(corridor: number, mapHeight: number | undefined): { x: number; y: number }[] | null {
  const floor = corridor + BLOCK_DEPTH;
  if (mapHeight === undefined || floor + KART_HEIGHT > mapHeight - WALL_INSET) return null;
  const gap = (column: number) => podOrigin(column).x + POD.width + Math.floor((podOrigin(column + 1).x - podOrigin(column).x - POD.width - FIGURE_WIDTH) / 2);
  const [near, far] = [gap(0), gap(1)];
  return [
    { x: far, y: corridor },
    { x: far, y: floor },
    { x: near, y: floor },
    { x: near, y: corridor },
  ];
}

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
export function planAntic(kind: AnticKind, pod: number, seed: number, floor: FloorNow = {}): AnticPlan {
  if (!ANTICS[kind].away) return { kind, to: null, act: [], seconds: ANTICS[kind].seconds };
  if (kind === "coffee") {
    return { kind, to: { ...coffeeSpot(seed, floor.taken ?? []), corridor: MAIN_CORRIDOR }, act: [], seconds: ANTICS.coffee.seconds };
  }
  const y = corridorOf(pod);
  const to: Spot = { x: deskOf(pod).x, y, corridor: y };
  const through = (points: { x: number; y: number }[], speed: number): Leg[] => {
    const stops = [to, ...points];
    return points.map((point, i) => ({ ...point, seconds: timed(Math.abs(point.x - stops[i].x) + Math.abs(point.y - stops[i].y), speed) })).filter((leg) => leg.seconds > 0);
  };
  const legs = (xs: number[], speed: number): Leg[] => through(xs.map((x) => ({ x, y })), speed);
  let act: Leg[] = [];
  if (kind === "kart") {
    // Four ways to take it out: up and down the corridor setting off either
    // way, or round the block below either way. Each turns where it likes.
    const way = seed % 4;
    const block = way >= 2 ? blockBelow(y, floor.mapHeight) : null;
    if (block) {
      const lap = way === 2 ? block : [...block].reverse();
      act = through([...Array.from({ length: LAP.laps }, () => lap).flat(), to], SPEED.kart);
    } else {
      const left = LAP.left + ((seed >>> 2) % 6) * LAP.give;
      const right = LAP.right - ((seed >>> 5) % 5) * LAP.give;
      const ends = way % 2 === 0 ? [left, right] : [right, left];
      act = legs([...Array.from({ length: LAP.laps }, () => ends).flat(), to.x], SPEED.kart);
    }
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
