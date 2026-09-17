/**
 * Who sits where, and what they are doing: the office's model, with no drawing
 * in it. Both views read this, so an agent is in the same pod in each.
 */

import type { AgentStatus } from "../../platform/generated/AgentStatus";

/** Pods to a row, in both views. */
export const PODS_PER_ROW = 3;
/** Rows the office has even when the limit would fit in fewer. */
const MIN_ROWS = 2;

/** Agent id to pod number, pods counted from 0 along the rows. */
export type Seating = ReadonlyMap<string, number>;

interface Placed {
  id: string;
  workspace_id: string;
  status: AgentStatus;
  started_at: number;
}

/** The living agents of a workspace, in the order they arrived. */
export function occupants<T extends Placed>(agents: readonly T[], workspaceId: string): T[] {
  return agents
    .filter((agent) => agent.workspace_id === workspaceId && agent.status !== "dead")
    .sort((a, b) => a.started_at - b.started_at || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/**
 * Seats `present` given where people sat a moment ago: whoever has a pod keeps
 * it, whoever left gives theirs up, and a newcomer takes the first vacant one.
 * An office that reshuffled every time the list changed would be unreadable.
 */
export function seat(previous: Seating, present: readonly { id: string }[]): Seating {
  const next = new Map<string, number>();
  for (const { id } of present) {
    const pod = previous.get(id);
    if (pod !== undefined) next.set(id, pod);
  }
  const taken = new Set(next.values());
  let vacant = 0;
  for (const { id } of present) {
    if (next.has(id)) continue;
    while (taken.has(vacant)) vacant += 1;
    next.set(id, vacant);
    taken.add(vacant);
  }
  return next;
}

/** How many pods to draw: whole rows, enough for the limit and for everyone seated. */
export function podCount(seating: Seating, maxConcurrent: number): number {
  const furthest = Math.max(-1, ...seating.values()) + 1;
  const needed = Math.max(maxConcurrent, furthest, MIN_ROWS * PODS_PER_ROW);
  return Math.ceil(needed / PODS_PER_ROW) * PODS_PER_ROW;
}

export interface Headcount {
  used: number;
  max: number;
  /** No seat left: a spawn would be refused. */
  full: boolean;
  text: string;
}

/** HR's seat meter. */
export function headcount(used: number, max: number): Headcount {
  const full = used >= max;
  return { used, max, full, text: full ? `Office full · ${used} / ${max}` : `${used} / ${max} seats` };
}

export type Pose = "typing" | "raised" | "still" | "error";

/** What an agent in this state is seen doing at its desk. */
export function poseOf(status: AgentStatus): Pose {
  switch (status) {
    case "running":
      return "typing";
    case "waiting":
      return "raised";
    case "error":
      return "error";
    default:
      return "still";
  }
}

/**
 * What HR says under its button: where the next hire would sit, or why there
 * cannot be one. `refused` is a hire the daemon turned down for want of a seat;
 * it stops mattering the moment a seat frees up.
 */
export function hrNote(seats: Headcount, pods: readonly unknown[], refused: boolean): string {
  const vacant = pods.findIndex((pod) => pod === null);
  if (!seats.full && vacant >= 0) return `Office ${vacant + 1} is free.`;
  return refused ? "Hiring freeze — every seat is taken." : "No seat until someone leaves.";
}
