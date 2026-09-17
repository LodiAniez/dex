/**
 * Who walks where: a new hire from HR's door to their pod, and whoever has
 * ended back the way they came. The rules only — what changed in the agent
 * list, the route across the floor, and the queue that keeps two people from
 * walking at once. Drawing and timers are `Walker.tsx`'s.
 */

import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { PODS_PER_ROW } from "./floor";
import { podOrigin } from "./mapGeometry";

interface Tracked {
  id: string;
  workspace_id: string;
  status: AgentStatus;
  parent_id: string | null;
  depth: number;
}

export interface Movement {
  /** A hire coming in, someone who has ended going out, or a message being carried to a desk. */
  kind: "arrive" | "leave" | "deliver";
  id: string;
}

/**
 * What the change from `before` to `after` means for one workspace's floor.
 * Departures first, so that a pod freed and refilled in one update is walked
 * out of before it is walked into. Nothing on the first load (`before` null):
 * opening Dex on a busy office must not march everyone in.
 *
 * Only a hire walks in — an agent with a parent, or spawned through Dex at all
 * (depth above 0). One the owner started by typing `claude` was always there.
 */
export function movements(before: readonly Tracked[] | null, after: readonly Tracked[], workspaceId: string): Movement[] {
  if (before === null) return [];
  const here = (agent: Tracked) => agent.workspace_id === workspaceId && agent.status !== "dead";
  const was = new Set(before.filter(here).map((agent) => agent.id));
  const now = new Set(after.filter(here).map((agent) => agent.id));
  const leaving = [...was].filter((id) => !now.has(id)).map((id): Movement => ({ kind: "leave", id }));
  const arriving = after
    .filter((agent) => here(agent) && !was.has(agent.id) && (agent.parent_id !== null || agent.depth > 0))
    .map((agent): Movement => ({ kind: "arrive", id: agent.id }));
  return [...leaving, ...arriving];
}

/** Where a walker stands when they step out of HR. A walker's position is their top-left. */
export const HR_DOOR = { x: 150, y: 240 } as const;
/**
 * The way out: a door in the left wall at the end of the main corridor, in the
 * stretch between HR's room and the break room. Whoever ends leaves by it. HR is
 * where people are hired, and nothing else.
 */
export const EXIT_DOOR = { x: 24, y: 400 } as const;
/** How long someone stands at the door waving before they go. */
export const WAVE_SECONDS = 1.4;
/** How long a new hire stands at the door before setting off. */
export const DOOR_PAUSE = 0.6;

/** A walker stops this far inside a pod's box. */
const INTO_POD = 80;
/** The first corridor, and how far down the next one is: there is one per pair of rows. */
const FIRST_CORRIDOR = 400;
const CORRIDOR_STEP = 700;
/** The design's pace: 160 units down in 0.9 s, 890 across in 1.9 s, 160 into the pod in 0.8 s. */
const PACE = { down: 0.9 / 160, across: 1.9 / 890, into: 0.8 / 160 } as const;

export interface Leg {
  x: number;
  y: number;
  /** How long this leg takes. */
  seconds: number;
}

/** From HR's door to pod `pod`: down to its corridor, along it, and in. Never through a desk. */
export function legsTo(pod: number): Leg[] {
  const origin = podOrigin(pod);
  const corridor = corridorOf(pod);
  const x = origin.x + INTO_POD;
  const y = origin.y + INTO_POD;
  // `timed` rounds to the millisecond: these become CSS durations, and 0.9000000000000001 s is nobody's friend.
  return [
    { x: HR_DOOR.x, y: corridor, seconds: timed(corridor - HR_DOOR.y, PACE.down) },
    { x, y: corridor, seconds: timed(x - HR_DOOR.x, PACE.across) },
    { x, y, seconds: timed(y - corridor, PACE.into) },
  ];
}

export interface Walk extends Movement {
  /** The walker's own pod: where a hire is going, and where everyone else starts. */
  pod: number;
  /** For a delivery, the pods to visit, in order. It can grow while the walk is under way. */
  stops?: number[];
  /** The messages behind those stops, so that none is delivered twice. */
  keys?: string[];
  /** What makes this walk itself and no other; without one, its kind and who walks it. */
  key?: string;
}

/** A visitor stands this far inside the pod they are visiting: beside the chair of whoever sits there, not on them or their desk. */
const VISIT_INTO_POD = { x: 30, y: 110 } as const;
/** How long a conversation at a colleague's desk lasts. */
export const TALK_SECONDS = 2.6;

const timed = (distance: number, pace: number) => Math.round(Math.abs(distance) * pace * 1000) / 1000;

function corridorOf(pod: number): number {
  return FIRST_CORRIDOR + Math.floor(Math.floor(pod / PODS_PER_ROW) / 2) * CORRIDOR_STEP;
}

interface Spot {
  x: number;
  y: number;
  /** The corridor this spot is reached from. */
  corridor: number;
}

/** Where someone sits: a walker standing here is at their own desk. */
export function deskOf(pod: number): Spot {
  return { x: podOrigin(pod).x + INTO_POD, y: podOrigin(pod).y + INTO_POD, corridor: corridorOf(pod) };
}

function besideDeskOf(pod: number): Spot {
  return { x: podOrigin(pod).x + VISIT_INTO_POD.x, y: podOrigin(pod).y + VISIT_INTO_POD.y, corridor: corridorOf(pod) };
}

/**
 * From one spot in a pod to another: out to the corridor, along it, and in.
 * Between corridors, by the aisle outside HR and never through a row of desks.
 */
function between(from: Spot, to: Spot): Leg[] {
  const stops = [{ x: from.x, y: from.y }, { x: from.x, y: from.corridor }];
  if (from.corridor !== to.corridor) stops.push({ x: HR_DOOR.x, y: from.corridor }, { x: HR_DOOR.x, y: to.corridor });
  stops.push({ x: to.x, y: to.corridor }, { x: to.x, y: to.y });
  return stops
    .slice(1)
    .map((stop, i) => ({ ...stop, seconds: stops[i].x !== stop.x ? timed(stop.x - stops[i].x, PACE.across) : timed(stop.y - stops[i].y, PACE.down) }))
    .filter((leg) => leg.seconds > 0);
}

/**
 * What someone on a round does next, decided one step at a time because the
 * round can grow while they are out: to the next colleague they have not told
 * yet, straight from wherever they are; home once everyone has been told; and
 * null when they are home with nobody left. `visited` is how many stops are
 * done; `where` is the pod they are standing in, or "home".
 */
export function nextLegs(
  walk: Pick<Walk, "pod" | "stops">,
  visited: number,
  where: number | "home",
): { legs: Leg[]; arrives: number | "home" } | null {
  const here = where === "home" ? deskOf(walk.pod) : besideDeskOf(where);
  const next = walk.stops?.[visited];
  if (next !== undefined) return { legs: between(here, besideDeskOf(next)), arrives: next };
  return where === "home" ? null : { legs: between(here, deskOf(walk.pod)), arrives: "home" };
}

/** Where a walk starts and the legs it takes: a hire's way in from HR, or a leaver's way out of the door. */
export function routeOf(walk: Pick<Walk, "kind" | "pod">): { from: { x: number; y: number }; legs: Leg[] } {
  if (walk.kind === "arrive") return { from: { ...HR_DOOR }, legs: legsTo(walk.pod) };
  // Out: from their desk to their corridor, up the side aisle if theirs is not
  // the main one, and along the main corridor to the door at its end.
  const desk = deskOf(walk.pod);
  const door = { ...EXIT_DOOR, corridor: EXIT_DOOR.y };
  return { from: { x: desk.x, y: desk.y }, legs: between(desk, door) };
}

/** A whole walk, door pause included. */
export function walkDuration(legs: readonly Leg[]): number {
  return DOOR_PAUSE + legs.reduce((total, leg) => total + leg.seconds, 0);
}

/**
 * Adds `walks` to the queue; the head of the queue is whoever is walking now.
 * The same walk is never queued twice. Someone who leaves before their arrival
 * has begun never walks at all; someone who leaves mid-walk finishes it and is
 * then seen out. `still` is someone who has asked for less motion: nobody walks.
 */
export function enqueue<T extends Walk>(queue: readonly T[], walks: readonly T[], options: { still?: boolean } = {}): T[] {
  if (options.still) return [];
  let next = [...queue];
  for (const walk of walks) {
    if (walk.kind === "deliver") {
      // Only what has not been delivered or queued already; and onto the round
      // its sender is already on if there is one, even if they have set off.
      const known = new Set(next.flatMap((queued) => queued.keys ?? []));
      const fresh = (walk.keys ?? []).map((key, i) => ({ key, stop: walk.stops?.[i] })).filter(({ key, stop }) => !known.has(key) && stop !== undefined);
      if (walk.keys && fresh.length === 0) continue;
      const stops = walk.keys ? fresh.map(({ stop }) => stop as number) : (walk.stops ?? []);
      const keys = fresh.map(({ key }) => key);
      const round = next.findIndex((queued) => queued.kind === "deliver" && queued.id === walk.id);
      if (round >= 0) {
        const onto = next[round];
        next[round] = { ...onto, stops: [...(onto.stops ?? []), ...stops], keys: [...(onto.keys ?? []), ...keys] };
      } else {
        next.push({ ...walk, stops, keys: walk.keys ? keys : walk.keys });
      }
      continue;
    }
    if (next.some((queued) => queued.id === walk.id && queued.kind === walk.kind)) continue;
    if (walk.kind === "leave") {
      const arriving = next.some((queued, i) => i > 0 && queued.id === walk.id && queued.kind === "arrive");
      // Whatever they had not yet set off on, they never will.
      next = next.filter((queued, i) => i === 0 || queued.id !== walk.id);
      if (arriving) continue;
    }
    next.push(walk);
  }
  return next;
}

interface MessageEvent {
  seq: number;
  kind: string;
  agent_id: string | null;
  target_agent_id: string | null;
}

/**
 * The messages newer than `sinceSeq` as rounds: each sender walks to everyone
 * they wrote to, in the order they wrote, and home at the end. Only between
 * people at desks here - a memo from the owner has nobody to carry it, and
 * nowhere to carry it from.
 */
export function deliveries(events: readonly MessageEvent[], sinceSeq: number, seats: ReadonlyMap<string, number>): Walk[] {
  const rounds = new Map<string, Walk>();
  for (const event of events) {
    if (event.kind !== "message" || event.seq <= sinceSeq) continue;
    const from = event.agent_id === null ? undefined : seats.get(event.agent_id);
    const to = event.target_agent_id === null ? undefined : seats.get(event.target_agent_id);
    if (event.agent_id === null || from === undefined || to === undefined || from === to) continue;
    const key = `m${event.seq}`;
    const round = rounds.get(event.agent_id);
    if (round) {
      round.stops?.push(to);
      round.keys?.push(key);
    } else {
      rounds.set(event.agent_id, { kind: "deliver", id: event.agent_id, pod: from, stops: [to], keys: [key], key });
    }
  }
  return [...rounds.values()];
}

/** Whoever is out delivering right now, whose desk should therefore be shown empty. */
export function awayFromDesk(queue: readonly Walk[]): string | null {
  return queue[0]?.kind === "deliver" ? queue[0].id : null;
}

/**
 * The queue once whoever was at its head has finished. `visited` is how many
 * stops a round made. If its round has grown past that - a message added in
 * the instant the walker decided it was done - they go out again for the
 * rest, rather than the message being dropped from view.
 */
export function afterWalk<T extends Walk>(queue: readonly T[], visited: number | undefined): T[] {
  const [head, ...rest] = queue;
  if (!head) return [];
  const stops = head.stops ?? [];
  if (head.kind !== "deliver" || visited === undefined || stops.length <= visited) return rest;
  const keys = (head.keys ?? []).slice(visited);
  return [{ ...head, stops: stops.slice(visited), keys, key: keys[0] ?? head.key }, ...rest];
}
