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
