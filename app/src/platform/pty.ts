import { Channel, invoke } from "@tauri-apps/api/core";

/** Non-output news about a pane's process. Mirrors `PtyEvent` in src-tauri/src/pty.rs. */
export type PtyEvent =
  | { type: "dropped"; bytes: number }
  | { type: "exited"; code: number | null };

export interface SpawnOptions {
  paneId: string;
  workspaceId?: string;
  cwd?: string;
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
      cols: options.cols,
      rows: options.rows,
    },
    onOutput,
    onEvent,
  });
}

export const writePty = (paneId: string, data: string) => invoke<void>("pty_write", { paneId, data });

export const resizePty = (paneId: string, cols: number, rows: number) =>
  invoke<void>("pty_resize", { paneId, cols, rows });

export const ackPty = (paneId: string, bytes: number) => invoke<void>("pty_ack", { paneId, bytes });

export const killPty = (paneId: string) => invoke<void>("pty_kill", { paneId });
