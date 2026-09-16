import { DexError } from "./daemon";

/** A transient message at the bottom of the window. */
export interface Notice {
  id: number;
  message: string;
  repair?: string;
}

const NOTICE_MS = 8000;

let notices: Notice[] = [];
let nextId = 0;
const listeners = new Set<() => void>();

function publish(next: Notice[]): void {
  notices = next;
  for (const listener of listeners) listener();
}

/** Shows an error, with its repair string when the daemon supplied one. */
export function showError(err: unknown): void {
  nextId += 1;
  const id = nextId;
  const notice: Notice =
    err instanceof DexError ? { id, message: err.message, repair: err.repair } : { id, message: String(err) };
  publish([...notices, notice]);
  window.setTimeout(() => dismissNotice(id), NOTICE_MS);
}

/**
 * Shows something the owner should know about but that stopped nothing — a
 * setting Dex could not use, and what it used instead.
 */
export function showWarning(message: string): void {
  nextId += 1;
  const id = nextId;
  publish([...notices, { id, message }]);
  window.setTimeout(() => dismissNotice(id), NOTICE_MS);
}

export function dismissNotice(id: number): void {
  if (notices.some((n) => n.id === id)) publish(notices.filter((n) => n.id !== id));
}

export function subscribeNotices(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getNotices(): Notice[] {
  return notices;
}
