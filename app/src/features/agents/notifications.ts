import { invoke } from "@tauri-apps/api/core";
import type { AgentList } from "../../platform/generated/AgentList";
import type { AgentStatus } from "../../platform/generated/AgentStatus";

/** Transitions worth a toast, and how to say them. */
const WORTH_A_TOAST: Partial<Record<AgentStatus, string>> = {
  waiting: "needs your input",
  idle: "finished",
  error: "stopped with an error",
};

/** Where a pane is, and whether the user is looking at it right now. */
export interface PaneContext {
  workspaceId: string;
  workspaceName: string;
  paneName: string;
  looking: boolean;
}

/**
 * Toasts for agents that just started needing attention (PRD §9.5): moving
 * into waiting, idle, or error, in a pane the user is not looking at. A pane
 * the user is looking at never notifies — that is pure noise. Clicking a
 * toast brings Dex forward on that pane (see `dex://focus-pane`).
 *
 * `nameOf` is what the office calls an agent. The title always says which
 * pane, so an agent nobody has named is still "Claude".
 */
export function notifyTransitions(
  before: AgentList | null,
  after: AgentList,
  locate: (paneId: string) => PaneContext | null,
  nameOf: (agentId: string) => string | undefined = () => undefined,
): void {
  // The first load reports current states, not things that just happened.
  if (!before) return;
  const previous = new Map(before.agents.map((agent) => [agent.id, agent.status]));
  for (const agent of after.agents) {
    const verb = WORTH_A_TOAST[agent.status];
    const was = previous.get(agent.id);
    // A brand-new agent starts idle; that is not "finished".
    if (!verb || !agent.pane_id || was === undefined || was === agent.status) continue;
    const pane = locate(agent.pane_id);
    if (!pane || pane.looking) continue;
    const detail = agent.status === "error" && agent.status_detail ? `: ${agent.status_detail}` : "";
    void invoke("notify_agent", {
      title: `${pane.workspaceName} · ${pane.paneName}`,
      body: `${nameOf(agent.id) ?? "Claude"} ${verb}${detail}`,
      workspace: pane.workspaceId,
      pane: agent.pane_id,
    }).catch((err) => console.warn("could not show a notification", err));
  }
}
