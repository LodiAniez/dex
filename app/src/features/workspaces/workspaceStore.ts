import { useSyncExternalStore } from "react";
import { request } from "../../platform/daemon";
import { onDaemonChange } from "../../platform/events";
import type { Layout } from "../../platform/generated/Layout";
import type { WorkspaceList } from "../../platform/generated/WorkspaceList";
import { showError } from "../../platform/notices";
import { disposeTerminal } from "../../platform/terminalRegistry";

/**
 * The workspace list as the daemon last reported it. Every workspace command
 * answers with the full list, so this store only ever replaces its snapshot.
 *
 * Two rules keep it honest:
 * - Responses can arrive out of order (Tauri runs commands concurrently), so a
 *   snapshot older than the current one — lower `revision` — is ignored.
 * - A pane that disappears from the list, however it went (closed here, from
 *   the CLI, or with its workspace), has its terminal disposed.
 */
let snapshot: WorkspaceList | null = null;
const listeners = new Set<() => void>();
let watching = false;

function publish(next: WorkspaceList): void {
  const before = snapshot;
  if (before && next.revision < before.revision) return;
  snapshot = next;
  for (const listener of listeners) listener();
  if (!before) return;
  const alive = new Set(next.workspaces.flatMap((ws) => ws.panes.map((pane) => pane.id)));
  for (const ws of before.workspaces) {
    for (const pane of ws.panes) {
      if (!alive.has(pane.id)) disposeTerminal(pane.id);
    }
  }
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

async function refresh(): Promise<void> {
  publish(await request<WorkspaceList>("workspace.list"));
}

/**
 * Loads saved workspaces (a fresh install gets a first workspace at the home
 * folder) and follows changes made elsewhere, such as from the `dex` CLI.
 */
export async function loadWorkspaces(): Promise<void> {
  if (!watching) {
    watching = true;
    void onDaemonChange("workspaces", () => void refresh().catch(showError));
  }
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

/** Deletes a workspace; its terminals are disposed as its panes leave the list. */
export async function deleteWorkspace(id: string): Promise<void> {
  publish(await request<WorkspaceList>("workspace.delete", { workspace: id }));
}

/** Splits a pane; the new pane starts in the same folder and takes focus. */
export async function splitPane(
  pane: string,
  direction: "right" | "down",
  // `markdown` is missing on purpose: it needs a file, which only the CLI supplies.
  kind?: "terminal" | "activity" | "diff",
  /** `windows` or `wsl:<distro>`; the split pane's when absent. */
  runtime?: string,
): Promise<void> {
  publish(await request<WorkspaceList>("pane.split", { pane, direction, kind, runtime }));
}

/** Closes a pane. The daemon refuses to close a workspace's last pane. */
export async function closePane(pane: string): Promise<void> {
  publish(await request<WorkspaceList>("pane.close", { pane }));
}

/** Focuses a pane immediately on screen, then records it. */
export async function focusPane(pane: string): Promise<void> {
  if (snapshot) {
    publish({
      ...snapshot,
      workspaces: snapshot.workspaces.map((ws) =>
        ws.panes.some((p) => p.id === pane) ? { ...ws, active_pane: pane } : ws,
      ),
    });
  }
  publish(await request<WorkspaceList>("pane.focus", { pane }));
}

export async function swapPanes(a: string, b: string): Promise<void> {
  publish(await request<WorkspaceList>("pane.swap", { a, b }));
}

/** Moves a pane dropped on another: beside it on `side`, or in its place. */
export async function movePane(pane: string, target: string, side: "left" | "right" | "top" | "bottom" | "center"): Promise<void> {
  publish(await request<WorkspaceList>("pane.move", { pane, target, side }));
}

/** Saves a rearranged tree (after a divider drag). */
export async function setLayout(workspace: string, layout: Layout): Promise<void> {
  publish(await request<WorkspaceList>("workspace.set_layout", { workspace, layout }));
}

/** Rebuilds the workspace as the next of the five preset layouts. */
export async function cycleLayout(workspace: string): Promise<void> {
  publish(await request<WorkspaceList>("workspace.cycle_layout", { workspace }));
}
