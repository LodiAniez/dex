/**
 * **TERMINALS LIVE HERE, OUTSIDE REACT. DO NOT MOVE THEM INTO COMPONENT STATE.**
 *
 * Every xterm.js `Terminal` is created once per pane and lives in this map for
 * the pane's whole life. Switching workspaces only *moves* a terminal's element
 * between the visible layout and an offscreen parking host; the `Terminal` and
 * its PTY keep running (PRD §7.3). If a `Terminal` were created in a
 * `useEffect` and disposed in its cleanup, every workspace switch would kill
 * the user's agents. React owns the layout boxes; this module owns terminals.
 *
 * **The WebGL addon is the one exception.** Chromium caps live WebGL contexts
 * per page (~16) and silently drops the oldest beyond that, and three
 * workspaces of six panes is already over. So a terminal's WebGL addon is
 * disposed when it is detached and a fresh one loaded when it is shown. The
 * `Terminal` stays alive; only its renderer is recycled.
 */
import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import { ackPty, killPty, resizePty, spawnPty, writePty, type PtyEvent } from "./pty";

/** Acknowledge rendered output in batches of this size... */
const ACK_BATCH_BYTES = 64 * 1024;
/** ...or after this long, so a small trailing amount is never left unacknowledged. */
const ACK_FLUSH_MS = 50;
/** PRD §7.1: ConPTY repaints on every resize, so only send the settled size. */
const RESIZE_DEBOUNCE_MS = 100;

interface Entry {
  paneId: string;
  workspaceId?: string;
  cwd?: string;
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
}

const entries = new Map<string, Entry>();
let parkingHost: HTMLDivElement | null = null;
let shortcutFilter: ((event: KeyboardEvent) => boolean) | null = null;

/**
 * Lets the shell claim its keyboard shortcuts before xterm.js sees them;
 * otherwise xterm.js would send them to the shell as input.
 */
export function setShortcutFilter(filter: (event: KeyboardEvent) => boolean): void {
  shortcutFilter = filter;
}

/** Offscreen home for detached terminals: in the DOM, invisible, never resized. */
function parking(): HTMLDivElement {
  if (!parkingHost) {
    parkingHost = document.createElement("div");
    parkingHost.className = "terminal-parking";
    document.body.appendChild(parkingHost);
  }
  return parkingHost;
}

/** Creates the pane's terminal if it does not exist yet. Idempotent. */
export function openTerminal(paneId: string, cwd?: string, workspaceId?: string): void {
  if (entries.has(paneId)) return;

  const term = new Terminal({
    fontFamily: '"Cascadia Mono", Consolas, monospace',
    fontSize: 13,
    scrollback: 10000,
    cursorBlink: true,
    theme: { background: "#16181d", foreground: "#d7dae0" },
  });
  const fit = new FitAddon();
  const serialize = new SerializeAddon();
  term.loadAddon(fit);
  term.loadAddon(serialize);

  const element = document.createElement("div");
  element.className = "terminal-host";
  parking().appendChild(element);
  term.open(element);
  // Returning false makes xterm.js ignore the key; the event still bubbles to the shell's listener.
  term.attachCustomKeyEventHandler((event) => !(shortcutFilter?.(event) ?? false));

  const entry: Entry = {
    paneId, workspaceId, cwd, term, fit, serialize, element,
    webgl: null, spawned: false, dead: false, pendingAck: 0, ackTimer: null, resizeTimer: null,
    pendingInput: "", writing: false,
  };
  entries.set(paneId, entry);

  term.onData((data) => sendInput(entry, data));
  term.onResize(({ cols, rows }) => scheduleResize(entry, cols, rows));
}

/** Shows the pane's terminal inside `host`, loading a renderer and starting the PTY on first show. */
export function attachTerminal(paneId: string, host: HTMLElement): void {
  const entry = entries.get(paneId);
  if (!entry) return;
  host.appendChild(entry.element);
  loadWebgl(entry);
  entry.fit.fit();
  if (!entry.spawned) {
    entry.spawned = true;
    // Spawned only after the first fit, so the shell starts at the real size.
    void spawnPty({
      paneId,
      workspaceId: entry.workspaceId,
      cwd: entry.cwd,
      cols: entry.term.cols,
      rows: entry.term.rows,
      onData: (bytes) => writeOutput(entry, bytes),
      onEvent: (event) => handleEvent(entry, event),
    }).catch((err) => entry.term.write(`\r\n\x1b[31mCould not start a shell: ${err}\x1b[0m\r\n`));
  }
}

/** Gives the pane's terminal keyboard focus. */
export function focusTerminal(paneId: string): void {
  entries.get(paneId)?.term.focus();
}

/** Hides the pane's terminal without disposing it. Its PTY keeps running. */
export function detachTerminal(paneId: string): void {
  const entry = entries.get(paneId);
  if (!entry) return;
  entry.webgl?.dispose();
  entry.webgl = null;
  parking().appendChild(entry.element);
}

/** Refits the terminal to its host; the PTY is resized once the size settles. */
export function fitTerminal(paneId: string): void {
  entries.get(paneId)?.fit.fit();
}

/** Ends a pane for good: kills its process and disposes its terminal. Only on pane close. */
export function disposeTerminal(paneId: string): void {
  const entry = entries.get(paneId);
  if (!entry) return;
  if (!entry.dead) void killPty(paneId).catch(() => {});
  entry.term.dispose();
  entry.element.remove();
  entries.delete(paneId);
}

/** The pane's visible buffer as text-with-escapes (backs `dex pane capture`). */
export function captureTerminal(paneId: string): string | undefined {
  return entries.get(paneId)?.serialize.serialize();
}

function loadWebgl(entry: Entry): void {
  if (entry.webgl) return;
  try {
    const webgl = new WebglAddon();
    // Context loss is handled like a detach: drop the addon; the next attach loads a new one.
    webgl.onContextLoss(() => {
      webgl.dispose();
      if (entry.webgl === webgl) entry.webgl = null;
    });
    entry.term.loadAddon(webgl);
    entry.webgl = webgl;
  } catch (err) {
    // No WebGL: xterm.js keeps rendering with its DOM renderer, just slower.
    console.warn(`WebGL renderer unavailable for pane ${entry.paneId}`, err);
  }
}

/**
 * Sends keyboard input to the PTY, strictly in order. Tauri runs async
 * commands concurrently, so back-to-back `pty_write` calls can reach the PTY
 * out of order ("echo" arrives as "ehco"). One write is in flight per pane;
 * whatever is typed meanwhile goes out, batched, as the next write.
 */
function sendInput(entry: Entry, data: string): void {
  if (entry.dead) return;
  entry.pendingInput += data;
  if (!entry.writing) void flushInput(entry);
}

async function flushInput(entry: Entry): Promise<void> {
  entry.writing = true;
  try {
    while (entry.pendingInput && !entry.dead) {
      const chunk = entry.pendingInput;
      entry.pendingInput = "";
      try {
        await writePty(entry.paneId, chunk);
      } catch {
        // The process has exited; its exit notice already explains why input goes nowhere.
      }
    }
  } finally {
    entry.writing = false;
  }
}

/** Writes PTY output and acknowledges it once xterm.js has actually processed it (flow control). */
function writeOutput(entry: Entry, bytes: Uint8Array): void {
  entry.term.write(bytes, () => {
    entry.pendingAck += bytes.length;
    if (entry.pendingAck >= ACK_BATCH_BYTES) {
      flushAck(entry);
    } else if (entry.ackTimer === null) {
      entry.ackTimer = window.setTimeout(() => flushAck(entry), ACK_FLUSH_MS);
    }
  });
}

function flushAck(entry: Entry): void {
  if (entry.ackTimer !== null) {
    window.clearTimeout(entry.ackTimer);
    entry.ackTimer = null;
  }
  if (entry.pendingAck === 0 || entry.dead) return;
  const bytes = entry.pendingAck;
  entry.pendingAck = 0;
  void ackPty(entry.paneId, bytes).catch(() => {});
}

function scheduleResize(entry: Entry, cols: number, rows: number): void {
  if (entry.resizeTimer !== null) window.clearTimeout(entry.resizeTimer);
  entry.resizeTimer = window.setTimeout(() => {
    entry.resizeTimer = null;
    if (entry.spawned && !entry.dead) void resizePty(entry.paneId, cols, rows).catch(() => {});
  }, RESIZE_DEBOUNCE_MS);
}

function handleEvent(entry: Entry, event: PtyEvent): void {
  if (event.type === "dropped") {
    // Bytes were discarded mid-stream, so the terminal state is unknown: reset it (PRD §7.1).
    entry.term.reset();
    entry.term.write("\x1b[2m[… output dropped while the display was unresponsive …]\x1b[0m\r\n");
    return;
  }
  // Keep the pane and its scrollback: output after a crash is often exactly what the user wants.
  entry.dead = true;
  const code = event.code === null ? "unknown" : String(event.code);
  entry.term.write(`\r\n\x1b[2m[process exited with code ${code}]\x1b[0m\r\n`);
}
