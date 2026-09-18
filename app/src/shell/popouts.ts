/**
 * Panes in windows of their own, from the main window's side. The pane's
 * window asks for the terminal when its page is ready ("popout-ready"), is
 * sent the screen ("popout-state"), and hands it back when it is docked or
 * closed ("popout-dock"), after which it is told it may go ("popout-docked").
 * If it goes without handing anything back - a crash - "popout-closed" brings
 * the pane home anyway, with this window's own last screen.
 */

import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import { showError } from "../platform/notices";
import { handOff, takeOver } from "../platform/terminalRegistry";

export const READY = "popout-ready";
export const STATE = "popout-state";
export const DOCK = "popout-dock";
export const DOCKED = "popout-docked";
const CLOSED = "popout-closed";

export interface PaneMessage {
  paneId: string;
}

export interface Screen extends PaneMessage {
  content: string;
}

export const windowOf = (paneId: string) => `pane-${paneId}`;

let detached: ReadonlySet<string> = new Set();
const listeners = new Set<() => void>();
function setDetached(next: ReadonlySet<string>): void {
  detached = next;
  for (const listener of listeners) listener();
}

/** The panes in windows of their own right now. */
export function useDetached(): ReadonlySet<string> {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => detached,
  );
}

/** Whether the pane is in a window of its own right now. */
export function isDetached(paneId: string): boolean {
  return detached.has(paneId);
}

/** Brings a popped-out pane's window forward. */
export async function bringForward(paneId: string): Promise<void> {
  await invoke("pane_pop_out", { paneId, title: "" });
}

/** Where each pane came from, for bringing it home. */
const homes = new Map<string, { cwd: string; workspaceId: string }>();

/** Opens the pane in a window of its own, or brings that window forward. */
export async function popOut(pane: { id: string; cwd: string }, workspaceId: string, title: string): Promise<void> {
  if (detached.has(pane.id)) {
    await invoke("pane_pop_out", { paneId: pane.id, title });
    return;
  }
  homes.set(pane.id, { cwd: pane.cwd, workspaceId });
  // Listening before the window exists: it asks as soon as its page is up.
  const unlisten = await listen<PaneMessage>(READY, (event) => {
    if (event.payload.paneId !== pane.id) return;
    unlisten();
    void (async () => {
      const content = await handOff(pane.id);
      setDetached(new Set([...detached, pane.id]));
      await emitTo(windowOf(pane.id), STATE, { paneId: pane.id, content } satisfies Screen);
    })().catch(showError);
  });
  try {
    await invoke("pane_pop_out", { paneId: pane.id, title });
  } catch (err) {
    unlisten();
    throw err;
  }
}

async function comeHome(paneId: string, content: string | null): Promise<void> {
  if (!detached.has(paneId)) return;
  const home = homes.get(paneId);
  await takeOver(paneId, content, home?.cwd, home?.workspaceId);
  const next = new Set(detached);
  next.delete(paneId);
  setDetached(next);
}

/** Listens for panes coming home. Once, from the main window. */
export async function watchPopouts(): Promise<void> {
  await listen<Screen>(DOCK, (event) => {
    const { paneId, content } = event.payload;
    void comeHome(paneId, content)
      .then(() => emitTo(windowOf(paneId), DOCKED, { paneId } satisfies PaneMessage))
      .catch(showError);
  });
  await listen<string>(CLOSED, (event) => void comeHome(event.payload, null).catch(showError));
}

/** Forgets panes that no longer exist (closed from the CLI while out). */
export function forgetDetached(alive: ReadonlySet<string>): void {
  const kept = [...detached].filter((paneId) => alive.has(paneId));
  if (kept.length !== detached.size) setDetached(new Set(kept));
}
