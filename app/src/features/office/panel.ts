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

/** The daemon writes a hire as "<hirer's label> started an agent for: <brief>". */
const HIRED_BY = / started an agent for: /;

/**
 * What an agent has done, newest first: the notes it wrote, what it stored,
 * who it messaged, who it hired. Not its status flips - those say what state
 * it was in, not what it did, and there are twenty of them to every note.
 *
 * A hire is filed by the daemon under the agent that was hired, with the
 * hirer named in the body by label; `labels` are the labels this agent goes
 * by, so that its hires count as its own doing as well.
 */
export function doneBy<T extends { agent_id: string | null; kind: string; body: string }>(
  events: readonly T[] | undefined,
  agentId: string,
  labels: readonly string[] = [],
): T[] {
  const hiredByThem = (event: T) => event.kind === "spawn" && labels.includes(event.body.split(HIRED_BY)[0]);
  return (events ?? []).filter((event) => event.kind !== "status" && (event.agent_id === agentId || hiredByThem(event))).reverse();
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

/**
 * What an agent needs from the owner, in a sentence, or null when it needs
 * nothing. "Needs you" on its own sends them to the pane to find out what for;
 * the hook that raised it usually said, and the answer is always in the pane.
 */
export function needsYou(status: string, detail: string | null): string | null {
  switch (status) {
    // Idle with something to say: their turn ended on a question. A prompt answers it.
    case "idle":
      return detail?.startsWith("asked you: ") ? `Asked you: ${detail.slice("asked you: ".length)} Answer with Prompt, or in their pane.` : null;
    case "waiting":
      return detail ? `Waiting for you: ${detail}. Answer it in their pane.` : "Waiting for you to answer something in their pane.";
    case "error":
      return `Stopped: ${detail ?? "no reason given"}. Their pane says more.`;
    case "unknown":
      return "Nothing has been heard from them for a while. Look at their pane.";
    default:
      return null;
  }
}
