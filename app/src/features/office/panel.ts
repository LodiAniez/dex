/** What the work panel says about one employee, worked out from what is already loaded. */

/**
 * The newest `count` things this agent did, oldest first. By id: a label can be
 * shared, and the human's events have no id at all.
 */
export function eventsOf<T extends { agent_id: string | null }>(
  events: readonly T[] | undefined,
  agentId: string,
  count: number,
): T[] {
  return (events ?? []).filter((event) => event.agent_id === agentId).slice(-count);
}

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
