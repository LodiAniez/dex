/**
 * Prompting an agent from the office: typing into its terminal as the owner's
 * turn, which is not the same thing as a memo. A memo goes to the agent's inbox
 * and is read when the agent looks; a prompt is what the owner would have typed
 * had they gone to the pane. Announcing is a prompt to everyone at once.
 */

import type { AgentStatus } from "../../platform/generated/AgentStatus";

interface Promptable {
  status: AgentStatus;
  pane_id: string | null;
}

/** Arguments for `pane.send`, or null when nothing was written. */
export function promptArgs(paneId: string, text: string): { pane: string; text: string; enter: true } | null {
  // One line: in a terminal a newline is Enter, and half a prompt submitted
  // early is worse than a prompt with its line breaks flattened.
  const line = text.split(/\s*[\r\n]+\s*/).join(" ").trim();
  return line ? { pane: paneId, text: line, enter: true } : null;
}

/** Why this agent cannot be prompted right now, in a few words, or null if it can. */
export function whyNoPrompt(agent: Promptable): string | null {
  if (!agent.pane_id) return "no pane";
  // At a permission dialog typed text answers the dialog, not the agent.
  if (agent.status === "waiting") return "waiting for you";
  return null;
}

interface Member<A extends Promptable> {
  agent: A;
  persona: { name: string };
}

/** Who an announcement reaches, and who it cannot and why. */
export function announcePlan<A extends Promptable, M extends Member<A>>(staff: readonly M[]): { to: M[]; skipped: M[]; reasons: Map<M, string> } {
  const to: M[] = [];
  const skipped: M[] = [];
  const reasons = new Map<M, string>();
  for (const member of staff) {
    const why = whyNoPrompt(member.agent);
    if (why === null) to.push(member);
    else {
      skipped.push(member);
      reasons.set(member, why);
    }
  }
  return { to, skipped, reasons };
}

/** One sentence for the owner: who heard it, and who did not. */
export function announceSummary<A extends Promptable, M extends Member<A>>(plan: ReturnType<typeof announcePlan<A, M>>): string {
  const heard = plan.to.length === 0 ? "Nobody to announce to." : `Announced to ${plan.to.length} agent${plan.to.length === 1 ? "" : "s"}.`;
  if (plan.skipped.length === 0) return heard;
  const missed = plan.skipped.map((member) => `${member.persona.name} (${plan.reasons.get(member)})`);
  const list = missed.length === 1 ? missed[0] : `${missed.slice(0, -1).join(", ")} or ${missed[missed.length - 1]}`;
  return `${heard} Not to ${list}.`;
}
