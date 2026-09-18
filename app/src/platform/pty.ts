import { Channel, invoke } from "@tauri-apps/api/core";

/** Non-output news about a pane's process. Mirrors `PtyEvent` in src-tauri/src/pty.rs. */
export type PtyEvent =
  | { type: "dropped"; bytes: number }
  | { type: "exited"; code: number | null };

export interface SpawnOptions {
  paneId: string;
  workspaceId?: string;
  cwd?: string;
  /** Where the shell runs: `windows` (the default) or `wsl:<distro>`. */
  runtime?: string;
  cols: number;
  rows: number;
  onData: (bytes: Uint8Array) => void;
  onEvent: (event: PtyEvent) => void;
}

/** Starts the default shell for a pane. Output arrives as raw bytes on `onData`. */
export async function spawnPty(options: SpawnOptions): Promise<void> {
  const onOutput = new Channel<ArrayBuffer | number[]>();
  // Raw channel payloads arrive as ArrayBuffer; the array form is a fallback.
  onOutput.onmessage = (message) =>
    options.onData(message instanceof ArrayBuffer ? new Uint8Array(message) : Uint8Array.from(message));
  const onEvent = new Channel<PtyEvent>();
  onEvent.onmessage = options.onEvent;
  await invoke("pty_spawn", {
    pane: {
      paneId: options.paneId,
      workspaceId: options.workspaceId ?? null,
      cwd: options.cwd ?? null,
      runtime: options.runtime ?? null,
      cols: options.cols,
      rows: options.rows,
    },
    onOutput,
    onEvent,
  });
}

/** First half of moving a pane between windows: its output is kept. Returns the bytes this window was sent. */
export const holdPty = (paneId: string) => invoke<number>("pty_hold", { paneId });

/** Second half: this window takes the pane's output - what was kept, then the rest. */
export async function attachPty(paneId: string, onData: (bytes: Uint8Array) => void, onEventCb: (event: PtyEvent) => void): Promise<void> {
  const onOutput = new Channel<ArrayBuffer | number[]>();
  onOutput.onmessage = (message) => onData(message instanceof ArrayBuffer ? new Uint8Array(message) : Uint8Array.from(message));
  const onEvent = new Channel<PtyEvent>();
  onEvent.onmessage = onEventCb;
  await invoke("pty_attach", { paneId, onOutput, onEvent });
}

export const writePty = (paneId: string, data: string) => invoke<void>("pty_write", { paneId, data });

export const resizePty = (paneId: string, cols: number, rows: number) =>
  invoke<void>("pty_resize", { paneId, cols, rows });

export const ackPty = (paneId: string, bytes: number) => invoke<void>("pty_ack", { paneId, bytes });

export const killPty = (paneId: string) => invoke<void>("pty_kill", { paneId });
