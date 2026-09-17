/**
 * An agent that spawns others is sending to HR for staff, and says so at the
 * top of its voice. The rules: who has just been sent for and by whom, what is
 * shouted, and for how long. `Shouting.tsx` draws it.
 *
 * Hires are read off the agent list, not the activity log: a hire's `parent_id`
 * says who sent for them without anyone parsing a sentence.
 */

/** How long a shout hangs in the air after it was last heard. */
export const SHOUT_SECONDS = 4.5;
/** Hires this close together are one round of hiring, and the number shouted grows. A lead spawns one tool call at a time. */
export const BURST_SECONDS = 25;
/** How long a hire waits inside HR's door once the shout starts, so that the shout is seen to come first. */
export const HEAD_START_SECONDS = 1.6;

export interface Shout {
  /** The agent doing the shouting. */
  hirer: string;
  /** How many they have sent for in this round. */
  count: number;
  /** When they last sent for one, in milliseconds. */
  heardAt: number;
}

export function shoutText(count: number): string {
  return `I need ${count} ${count === 1 ? "engineer" : "engineers"} on the floor ASAP!`;
}

/**
 * Whoever has appeared since `known` was taken and was sent for by another
 * agent. `known` is null when the map has only just opened: whoever is there
 * was hired before anyone was watching.
 */
export function newHires(known: ReadonlySet<string> | null, agents: readonly { id: string; parent_id: string | null }[]): { id: string; hirer: string }[] {
  if (known === null) return [];
  return agents.flatMap((agent) => (agent.parent_id !== null && !known.has(agent.id) ? [{ id: agent.id, hirer: agent.parent_id }] : []));
}

/** The shout once `hirer` has sent for one more. */
export function hear(current: Shout | null, hirer: string, now: number): Shout {
  const sameRound = current !== null && current.hirer === hirer && now - current.heardAt <= BURST_SECONDS * 1000;
  return { hirer, count: sameRound ? current.count + 1 : 1, heardAt: now };
}

export function isShouting(shout: Shout | null, now: number): boolean {
  return shout !== null && now - shout.heardAt < SHOUT_SECONDS * 1000;
}

/** Whether a hire should wait inside HR a moment longer: the shout comes first. */
export function holdsTheDoor(shout: Shout | null, now: number): boolean {
  return shout !== null && now - shout.heardAt < HEAD_START_SECONDS * 1000;
}
