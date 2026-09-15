import { useSyncExternalStore } from "react";
import { request } from "../../platform/daemon";
import type { WorkspaceList } from "../../platform/generated/WorkspaceList";
import { disposeTerminal } from "../../platform/terminalRegistry";

/**
 * The workspace list as the daemon last reported it. Every workspace command
 * answers with the full list, so this store only ever replaces its snapshot.
 */
let snapshot: WorkspaceList | null = null;
const listeners = new Set<() => void>();

function publish(next: WorkspaceList): void {
  snapshot = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The current list, or null before the first load. For event handlers; components use `useWorkspaces`. */
export function getWorkspaces(): WorkspaceList | null {
  return snapshot;
}

/** The workspace list, re-rendering on every change. Null until loaded. */
export function useWorkspaces(): WorkspaceList | null {
  return useSyncExternalStore(subscribe, getWorkspaces);
}

/** Loads saved workspaces; a fresh install gets a first workspace at the home folder. */
export async function loadWorkspaces(): Promise<void> {
  const list = await request<WorkspaceList>("workspace.list");
  publish(list.workspaces.length > 0 ? list : await request<WorkspaceList>("workspace.create"));
}

export interface NewWorkspace {
  name?: string;
  rootPath?: string;
  color?: string;
}

export async function createWorkspace(fields: NewWorkspace): Promise<void> {
  publish(
    await request<WorkspaceList>("workspace.create", {
      name: fields.name,
      root_path: fields.rootPath,
      color: fields.color,
    }),
  );
}

/** Switches immediately on screen, then records it; PRD §14 M3 wants switching to feel instant. */
export async function switchWorkspace(id: string): Promise<void> {
  if (!snapshot || snapshot.active === id) return;
  publish({ ...snapshot, active: id });
  publish(await request<WorkspaceList>("workspace.switch", { workspace: id }));
}

export async function renameWorkspace(id: string, name: string): Promise<void> {
  publish(await request<WorkspaceList>("workspace.rename", { workspace: id, name }));
}

/** Sets a `#rrggbb` color, or clears it with null. */
export async function recolorWorkspace(id: string, color: string | null): Promise<void> {
  publish(await request<WorkspaceList>("workspace.recolor", { workspace: id, color }));
}

export async function reorderWorkspaces(order: string[]): Promise<void> {
  publish(await request<WorkspaceList>("workspace.reorder", { order }));
}

/** Deletes a workspace and ends its terminals for good. */
export async function deleteWorkspace(id: string): Promise<void> {
  const panes = snapshot?.workspaces.find((ws) => ws.id === id)?.panes ?? [];
  publish(await request<WorkspaceList>("workspace.delete", { workspace: id }));
  for (const pane of panes) disposeTerminal(pane.id);
}
