import { useSyncExternalStore } from "react";
import { request } from "../../platform/daemon";
import { onDaemonChange } from "../../platform/events";
import type { EventList } from "../../platform/generated/EventList";
import { showError } from "../../platform/notices";

/**
 * The workspace event log as the daemon last reported it, one snapshot per
 * workspace. Responses can arrive out of order, so a snapshot with a lower
 * `revision` than the one held is ignored.
 */
const snapshots = new Map<string, EventList>();
const listeners = new Set<() => void>();
/** Workspaces whose stream someone is watching, so a change refreshes them. */
const watched = new Set<string>();
let listening = false;

function publish(workspaceId: string, next: EventList): void {
  const before = snapshots.get(workspaceId);
  if (before && next.revision < before.revision) return;
  snapshots.set(workspaceId, next);
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The events of one workspace, re-rendering on every change. Null until loaded. */
export function useActivity(workspaceId: string): EventList | null {
  return useSyncExternalStore(subscribe, () => snapshots.get(workspaceId) ?? null);
}

async function refresh(workspaceId: string): Promise<void> {
  publish(workspaceId, await request<EventList>("context.events", { workspace: workspaceId }));
}

/**
 * Loads a workspace's events and follows every change the daemon announces.
 * Returns a function that stops watching when the last pane showing it closes.
 */
export function watchActivity(workspaceId: string): () => void {
  if (!listening) {
    listening = true;
    // One listener for every workspace: the topic does not say which changed,
    // and only the ones on screen are refreshed.
    void onDaemonChange("context", () => {
      for (const id of watched) void refresh(id).catch(showError);
    });
  }
  watched.add(workspaceId);
  void refresh(workspaceId).catch(showError);
  return () => {
    watched.delete(workspaceId);
  };
}
