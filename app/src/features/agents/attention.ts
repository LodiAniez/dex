/**
 * An agent that ends its turn by asking the owner something - "Reply yes and
 * I'll overwrite the file" - is idle to Claude Code, whose turn is over, and
 * waiting to the owner. The daemon keeps it `idle`, because answering it is an
 * ordinary prompt and a waiting agent refuses those; it says what was asked in
 * the status detail, which an idle agent has for no other reason. Everything
 * that *shows* a status shows `seenAs`; everything that decides whether an
 * agent may be typed at uses the status itself.
 */

import type { AgentStatus } from "../../platform/generated/AgentStatus";

interface Stated {
  status: AgentStatus;
  status_detail: string | null;
}

const ASKED = "asked you: ";

/** What an idle agent asked the owner as its turn ended, or null. */
export function askedYou(agent: Stated): string | null {
  if (agent.status !== "idle" || !agent.status_detail) return null;
  return agent.status_detail.startsWith(ASKED) ? agent.status_detail.slice(ASKED.length) : agent.status_detail;
}

/** The status the owner should see: someone who asked them something is waiting for them. */
export function seenAs(agent: Stated): AgentStatus {
  return askedYou(agent) === null ? agent.status : "waiting";
}
