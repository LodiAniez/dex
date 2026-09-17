import { useSyncExternalStore } from "react";
import { request } from "./daemon";
import { onDaemonChange } from "./events";
import type { ConfigView } from "./generated/ConfigView";
import { showError, showWarning } from "./notices";
import { buildKeymap, SHIPPED_KEYMAP, type Keymap } from "../shell/keybindings";

/**
 * The owner's settings, as far as the UI is concerned: the keymap, and the
 * few values the office draws from.
 *
 * Read once at startup and again whenever the daemon announces `config`, so an
 * edit to `config.toml` rebinds keys without a restart. Until the first answer
 * arrives the shipped bindings are in force — the app must be usable in the
 * moment before the daemon replies, and every default is right anyway.
 */
let keymap: Keymap = SHIPPED_KEYMAP;
const listeners = new Set<() => void>();

/** What the office needs to know; null until the daemon has answered once. */
export interface OfficeSettings {
  /** How many agents a workspace may have: the office's seats. */
  maxConcurrent: number;
  /** `[ui] view`: how a workspace is shown until the owner chooses. */
  officeView: string;
}
let office: OfficeSettings | null = null;

export function useOfficeSettings(): OfficeSettings | null {
  return useSyncExternalStore(subscribeKeymap, () => office);
}

/** The bindings in force. Read on every keystroke, so it must stay a lookup. */
export function currentKeymap(): Keymap {
  return keymap;
}

export function subscribeKeymap(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

async function refresh(): Promise<void> {
  const view = await request<ConfigView>("config.get", {});
  const built = buildKeymap(view.keys);
  keymap = built.keymap;
  if (office?.maxConcurrent !== view.max_concurrent || office.officeView !== view.view) {
    office = { maxConcurrent: view.max_concurrent, officeView: view.view };
  }
  for (const listener of listeners) listener();
  // The daemon reports what it could not use; this reports what the UI could
  // not. Both are the owner's own file talking back to them, so both are shown.
  for (const problem of [...view.problems, ...built.problems]) showWarning(problem);
}

/** Loads the settings and follows every change to them. */
export function watchConfig(): void {
  void refresh().catch(showError);
  void onDaemonChange("config", () => void refresh().catch(showError));
}
