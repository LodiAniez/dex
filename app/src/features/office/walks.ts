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
  kind: "arrive" | "leave";
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
  const corridor = FIRST_CORRIDOR + Math.floor(Math.floor(pod / PODS_PER_ROW) / 2) * CORRIDOR_STEP;
  const x = origin.x + INTO_POD;
  const y = origin.y + INTO_POD;
  // To the millisecond: these become CSS durations, and 0.9000000000000001 s is nobody's friend.
  const timed = (distance: number, pace: number) => Math.round(Math.abs(distance) * pace * 1000) / 1000;
  return [
    { x: HR_DOOR.x, y: corridor, seconds: timed(corridor - HR_DOOR.y, PACE.down) },
    { x, y: corridor, seconds: timed(x - HR_DOOR.x, PACE.across) },
    { x, y, seconds: timed(y - corridor, PACE.into) },
  ];
}

export interface Walk extends Movement {
  /** The pod walked to, or from. */
  pod: number;
}

/** Where a walk starts and the legs it takes: an arrival's route, or the same one backwards. */
export function routeOf(walk: Pick<Walk, "kind" | "pod">): { from: { x: number; y: number }; legs: Leg[] } {
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
  for (const walk of walks) {
    if (next.some((queued) => queued.id === walk.id && queued.kind === walk.kind)) continue;
    const waiting = next.findIndex((queued, i) => i > 0 && queued.id === walk.id && queued.kind === "arrive");
    if (walk.kind === "leave" && waiting > 0) {
      next = next.filter((_, i) => i !== waiting);
      continue;
    }
    next.push(walk);
  }
  return next;
}
