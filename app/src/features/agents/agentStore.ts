import { useSyncExternalStore } from "react";
import { request } from "../../platform/daemon";
import { onDaemonChange } from "../../platform/events";
import type { AgentList } from "../../platform/generated/AgentList";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import type { AgentView } from "../../platform/generated/AgentView";
import { showError } from "../../platform/notices";

/**
 * Agents as the daemon last reported them. Responses can arrive out of order,
 * so a snapshot with a lower `revision` than the current one is ignored.
 */
let snapshot: AgentList | null = null;
const listeners = new Set<() => void>();
const watchers = new Set<(before: AgentList | null, after: AgentList) => void>();
let watching = false;

function publish(next: AgentList): void {
  const before = snapshot;
  if (before && next.revision < before.revision) return;
  snapshot = next;
  for (const listener of listeners) listener();
  for (const watcher of watchers) watcher(before, next);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getAgents(): AgentList | null {
  return snapshot;
}

/** The live agents, re-rendering on every change. Null until loaded. */
export function useAgents(): AgentList | null {
  return useSyncExternalStore(subscribe, getAgents);
}

/** Calls `watcher` with the previous and new list on every change (for notifications). */
export function watchAgentChanges(watcher: (before: AgentList | null, after: AgentList) => void): () => void {
  watchers.add(watcher);
  return () => {
    watchers.delete(watcher);
  };
}

async function refresh(): Promise<void> {
  publish(await request<AgentList>("agent.list", {}));
}

/** Loads the agents and follows every change the daemon announces. */
export async function loadAgents(): Promise<void> {
  if (!watching) {
    watching = true;
    void onDaemonChange("agents", () => void refresh().catch(showError));
  }
  await refresh();
}

/** The agent running in `paneId`, if any: the newest one that has not ended. */
export function agentInPane(list: AgentList | null, paneId: string): AgentView | undefined {
  return list?.agents.find((agent) => agent.pane_id === paneId && agent.status !== "dead");
}

/** Statuses that need a human, most urgent first. */
const ATTENTION: AgentStatus[] = ["waiting", "error", "unknown"];

/** The most urgent status needing attention in a workspace, if any (the sidebar's dot). */
export function workspaceAttention(list: AgentList | null, workspaceId: string): AgentStatus | null {
  const statuses = new Set(list?.agents.filter((a) => a.workspace_id === workspaceId).map((a) => a.status));
  return ATTENTION.find((status) => statuses.has(status)) ?? null;
}

/** How many agents are running and waiting across all workspaces (the title bar's counts). */
export function agentCounts(list: AgentList | null): Record<"running" | "waiting" | "error", number> {
  const count = (status: AgentStatus) => list?.agents.filter((a) => a.status === status).length ?? 0;
  return { running: count("running"), waiting: count("waiting"), error: count("error") };
}
