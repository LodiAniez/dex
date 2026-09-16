import { invoke } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";

/** A release newer than this build, as the app reported it. */
export interface Update {
  version: string;
  url: string;
}

/**
 * How often to ask. Startup, then every six hours: releases are rare, GitHub's
 * unauthenticated API allows sixty calls an hour, and a pill that appears
 * within a working day of a release is soon enough.
 */
const EVERY_MS = 6 * 60 * 60 * 1000;
/** A little after startup, so the check never competes with the first paint. */
const FIRST_MS = 8000;

let update: Update | null = null;
const listeners = new Set<() => void>();

function publish(next: Update | null): void {
  if (next?.version === update?.version) return;
  update = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The available update, re-rendering when one appears. Null when current. */
export function useUpdate(): Update | null {
  return useSyncExternalStore(subscribe, () => update);
}

async function check(): Promise<void> {
  try {
    publish(await invoke<Update | null>("update_check"));
  } catch (err) {
    // Not news: an update check that failed is not something the title bar
    // should say anything about.
    console.debug("update check failed", err);
  }
}

/** Starts the periodic check. Call once. */
export function watchUpdates(): void {
  window.setTimeout(() => void check(), FIRST_MS);
  window.setInterval(() => void check(), EVERY_MS);
}

/** Opens the release page in the browser. */
export function openUpdate(found: Update): Promise<void> {
  return invoke("update_open", { url: found.url });
}
