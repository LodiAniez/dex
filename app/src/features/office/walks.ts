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
  /** For a delivery, the pod being visited. */
  to?: number;
  /** What makes this walk itself and no other; without one, its kind and who walks it. */
  key?: string;
}

/** A visitor stands this far inside the pod they are visiting: beside the chair of whoever sits there, not on them or their desk. */
const VISIT_INTO_POD = { x: 30, y: 110 } as const;
/** How long a message takes to hand over. */
export const VISIT_SECONDS = 1;

const timed = (distance: number, pace: number) => Math.round(Math.abs(distance) * pace * 1000) / 1000;

function corridorOf(pod: number): number {
  return FIRST_CORRIDOR + Math.floor(Math.floor(pod / PODS_PER_ROW) / 2) * CORRIDOR_STEP;
}

/**
 * From a desk to a colleague's and back. Out to the corridor, along it, and in
 * beside them; if they sit off another corridor, by the aisle outside HR and
 * never through a row of desks. Then the same way home.
 */
function visit(fromPod: number, toPod: number): { from: { x: number; y: number }; legs: Leg[] } {
  const home = { x: podOrigin(fromPod).x + INTO_POD, y: podOrigin(fromPod).y + INTO_POD };
  const there = { x: podOrigin(toPod).x + VISIT_INTO_POD.x, y: podOrigin(toPod).y + VISIT_INTO_POD.y };
  const [near, far] = [corridorOf(fromPod), corridorOf(toPod)];
  const stops = [home, { x: home.x, y: near }];
  if (near !== far) stops.push({ x: HR_DOOR.x, y: near }, { x: HR_DOOR.x, y: far });
  stops.push({ x: there.x, y: far }, there);
  const leg = (a: { x: number; y: number }, b: { x: number; y: number }): Leg => ({
    ...b,
    seconds: a.x !== b.x ? timed(b.x - a.x, PACE.across) : timed(b.y - a.y, PACE.down),
  });
  const out = stops.slice(1).map((stop, i) => leg(stops[i], stop));
  const back = [...stops].reverse();
  const home_again = back.slice(1).map((stop, i) => leg(back[i], stop));
  return { from: home, legs: [...out, { ...there, seconds: VISIT_SECONDS }, ...home_again] };
}

/** Where a walk starts and the legs it takes: an arrival's route, or the same one backwards. */
export function routeOf(walk: Pick<Walk, "kind" | "pod" | "to">): { from: { x: number; y: number }; legs: Leg[] } {
  if (walk.kind === "deliver") return visit(walk.pod, walk.to ?? walk.pod);
  const legs = legsTo(walk.pod);
  if (walk.kind === "arrive") return { from: { ...HR_DOOR }, legs };
  const stops = [{ ...HR_DOOR }, ...legs.map(({ x, y }) => ({ x, y }))];
  const back = legs.map((leg, i) => ({ ...stops[i], seconds: leg.seconds })).reverse();
  const last = legs[legs.length - 1];
  return { from: { x: last.x, y: last.y }, legs: back };
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
  const same = (a: Walk, b: Walk) => (a.key !== undefined || b.key !== undefined ? a.key === b.key : a.id === b.id && a.kind === b.kind);
  for (const walk of walks) {
    if (next.some((queued) => same(queued, walk))) continue;
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
 * The messages newer than `sinceSeq` as trips: the sender walks to the
 * recipient's desk and back, one trip per message. Only between two people at
 * desks here - a memo from the owner has nobody to carry it, and nowhere to
 * carry it from.
 */
export function deliveries(events: readonly MessageEvent[], sinceSeq: number, seats: ReadonlyMap<string, number>): Walk[] {
  const walks: Walk[] = [];
  for (const event of events) {
    if (event.kind !== "message" || event.seq <= sinceSeq) continue;
    const from = event.agent_id === null ? undefined : seats.get(event.agent_id);
    const to = event.target_agent_id === null ? undefined : seats.get(event.target_agent_id);
    if (event.agent_id === null || from === undefined || to === undefined || from === to) continue;
    walks.push({ kind: "deliver", id: event.agent_id, pod: from, to, key: `m${event.seq}` });
  }
  return walks;
}

/** Whoever is out delivering right now, whose desk should therefore be shown empty. */
export function awayFromDesk(queue: readonly Walk[]): string | null {
  return queue[0]?.kind === "deliver" ? queue[0].id : null;
}
