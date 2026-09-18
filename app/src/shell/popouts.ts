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
import { focusPane, getWorkspaces } from "../features/workspaces";
import { showError } from "../platform/notices";
import { handOff, takeOver } from "../platform/terminalRegistry";
import { visibleActive } from "./visibleLayout";

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

/** The panes in windows of their own, outside React. */
export function getDetached(): ReadonlySet<string> {
  return detached;
}

/** Whether the pane is in a window of its own right now. */
export function isDetached(paneId: string): boolean {
  return detached.has(paneId);
}

/**
 * Brings a popped-out pane's window forward. If it has gone without this
 * window hearing of it, the pane is brought home instead of opening an empty one.
 */
export async function bringForward(paneId: string): Promise<void> {
  const there = await invoke<boolean>("pane_focus_popout", { paneId });
  if (!there) await comeHome(paneId, null);
}

/** Panes whose window is opening: a second press waits for the first. */
const opening = new Set<string>();
/** How long a window has to ask for its pane before the press is forgotten. */
const OPEN_WAIT_MS = 15000;

/** The main window's focus leaves a pane that has gone to a window of its own. */
async function focusAway(paneId: string): Promise<void> {
  const workspace = getWorkspaces()?.workspaces.find((ws) => ws.panes.some((pane) => pane.id === paneId));
  if (!workspace?.layout || workspace.active_pane !== paneId) return;
  const next = visibleActive(workspace.layout, detached, paneId);
  if (next) await focusPane(next);
}

/** Where each pane came from, for bringing it home. */
const homes = new Map<string, { cwd: string; workspaceId: string }>();

/** Opens the pane in a window of its own, or brings that window forward. */
export async function popOut(pane: { id: string; cwd: string }, workspaceId: string, title: string): Promise<void> {
  if (detached.has(pane.id) || opening.has(pane.id)) {
    await invoke("pane_pop_out", { paneId: pane.id, title });
    return;
  }
  opening.add(pane.id);
  homes.set(pane.id, { cwd: pane.cwd, workspaceId });
  // Listening before the window exists: it asks as soon as its page is up.
  let done = false;
  const unlisten = await listen<PaneMessage>(READY, (event) => {
    if (event.payload.paneId !== pane.id || done) return;
    done = true;
    stop();
    void (async () => {
      const content = await handOff(pane.id);
      setDetached(new Set([...detached, pane.id]));
      await emitTo(windowOf(pane.id), STATE, { paneId: pane.id, content } satisfies Screen);
      await focusAway(pane.id);
    })().catch(showError);
  });
  const timer = window.setTimeout(() => stop(), OPEN_WAIT_MS);
  function stop() {
    window.clearTimeout(timer);
    unlisten();
    opening.delete(pane.id);
  }
  try {
    await invoke("pane_pop_out", { paneId: pane.id, title });
  } catch (err) {
    stop();
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
