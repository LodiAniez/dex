/**
 * The terminals this window holds, one per pane, and what is kept about each.
 * `terminalRegistry.ts` owns their lives; this is the map, so that the few
 * modules working beside it (`terminalMirror.ts`) can read it too.
 */
import type { FitAddon } from "@xterm/addon-fit";
import type { SerializeAddon } from "@xterm/addon-serialize";
import type { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";
import { type MirrorFeed, endAll } from "./mirrorFeed";

export interface Entry {
  paneId: string;
  workspaceId?: string;
  cwd?: string;
  runtime?: string;
  /** Closed by `restartIn`, to start again in its new runtime on exit. */
  switching: boolean;
  term: Terminal;
  fit: FitAddon;
  serialize: SerializeAddon;
  webgl: WebglAddon | null;
  element: HTMLDivElement;
  spawned: boolean;
  dead: boolean;
  pendingAck: number;
  ackTimer: number | null;
  resizeTimer: number | null;
  /** Input typed while a write was in flight; sent as the next write. */
  pendingInput: string;
  writing: boolean;
  /** Output bytes this window has been sent since it took the pane: what a hand-off waits for. */
  received: number;
  /** In another window: this one neither sends input nor resizes the PTY. */
  away: boolean;
  /** Just taken back from another window: the PTY is still at that window's size. */
  resizeOnShow: boolean;
  /** Mirrors watching the pane - the office's monitor (`terminalMirror.ts`). */
  mirrors: Set<MirrorFeed>;
  /** The output as text, for mirrors: kept across chunks, which can split a character. */
  decoder: TextDecoder;
}

export const entries = new Map<string, Entry>();

/**
 * Takes the pane's terminal off the map, then ends its mirrors - in that
 * order: a mirror told the pane ended looks for it again, and must not find it.
 */
export function retire(paneId: string): Entry | undefined {
  const entry = entries.get(paneId);
  if (!entry) return undefined;
  entries.delete(paneId);
  endAll(entry.mirrors);
  return entry;
}
