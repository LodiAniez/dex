/** What the work panel says about one employee, worked out from what is already loaded. */

interface Member {
  agent: { id: string; parent_id: string | null };
  persona: { name: string };
}

/** Who this employee answers to, by the name the office knows them by. */
export function reportsTo(employee: Member, staff: readonly Member[]): string {
  const parent = employee.agent.parent_id;
  if (parent === null) return "you";
  return staff.find((other) => other.agent.id === parent)?.persona.name ?? "someone who has left";
}

/**
 * What an agent has done, newest first: the notes it wrote, what it stored,
 * who it messaged, who it hired. Not its status flips - those say what state
 * it was in, not what it did, and there are twenty of them to every note.
 */
export function doneBy<T extends { agent_id: string | null; kind: string }>(events: readonly T[] | undefined, agentId: string): T[] {
  return (events ?? []).filter((event) => event.agent_id === agentId && event.kind !== "status").reverse();
}

/** How long ago, in the largest unit that fits. */
export function ago(at: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86_400)}d ago`;
}
