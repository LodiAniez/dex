/**
 * Where someone out of their cubicle was last seen. An agent that ends while
 * it is out fooling around walks to the exit from there, not from a desk it
 * was not sitting at. Kept outside React: whoever draws them out fooling around
 * is unmounted in the same breath as they leave.
 */

import type { Spot } from "./walks";

const seen = new Map<string, Spot>();

export function noteOut(agentId: string, spot: Spot): void {
  seen.set(agentId, spot);
}

export function noteHome(agentId: string): void {
  seen.delete(agentId);
}

/** Where they were, if not at their desk. Asked once: they are leaving. */
export function takeWhereabouts(agentId: string): Spot | null {
  const spot = seen.get(agentId) ?? null;
  seen.delete(agentId);
  return spot;
}

export function forgetWhereabouts(): void {
  seen.clear();
}
