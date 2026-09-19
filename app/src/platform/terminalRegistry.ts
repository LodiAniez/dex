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
import { joinWrapped, type BufferRow } from "./bufferText";
import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import { ackPty, attachPty, holdPty, killPty, resizePty, spawnPty, writePty, type PtyEvent } from "./pty";
import { runtimeLabel, switchNow } from "./runtimes";

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
  runtime?: string;
  /** Closed by `syncRuntime`, to start again in its new runtime on exit. */
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
export function openTerminal(paneId: string, cwd?: string, workspaceId?: string, runtime?: string): void {
  if (entries.has(paneId)) return;

  const term = new Terminal({
    fontFamily: '"Cascadia Mono", Consolas, Menlo, monospace',
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
    paneId, workspaceId, cwd, runtime, term, fit, serialize, element,
    webgl: null, spawned: false, dead: false, pendingAck: 0, ackTimer: null, resizeTimer: null,
    pendingInput: "", writing: false, received: 0, away: false, resizeOnShow: false, switching: false,
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
  // xterm reports a size only when it changes; a terminal back from another
  // window may fit to what it had before, while the PTY is at the other size.
  if (entry.resizeOnShow && !entry.dead) {
    entry.resizeOnShow = false;
    void resizePty(paneId, entry.term.cols, entry.term.rows).catch(() => {});
  }
  // Spawned only after the first fit, so the shell starts at the real size.
  if (!entry.spawned) startShell(entry);
}

function startShell(entry: Entry): void {
  entry.spawned = true;
  entry.dead = false;
  void spawnPty({
    paneId: entry.paneId,
    workspaceId: entry.workspaceId,
    cwd: entry.cwd,
    runtime: entry.runtime,
    cols: entry.term.cols,
    rows: entry.term.rows,
    onData: (bytes) => writeOutput(entry, bytes),
    onEvent: (event) => handleEvent(entry, event),
  }).catch((err) => entry.term.write(`\r\n\x1b[31mCould not start a shell: ${err}\x1b[0m\r\n`));
}

/**
 * The pane's runtime as the daemon has it now: where a shell not started yet
 * will start. A running shell is never moved from here - only by `restartIn`,
 * when the owner asks.
 */
export function syncRuntime(paneId: string, runtime: string): void {
  const entry = entries.get(paneId);
  if (entry && !entry.spawned) entry.runtime = runtime;
}

/** Whether this window can restart the pane's shell in `runtime`. */
export function canRestartIn(paneId: string, runtime: string): boolean {
  const entry = entries.get(paneId);
  return entry !== undefined && switchNow(entry, runtime);
}

/**
 * Closes the pane's shell and starts it again in `runtime`, in the pane's
 * folder, keeping the scrollback - for a pane the owner chose to move to a new
 * terminal. Whatever ran in the shell ends. False if this window cannot.
 */
export function restartIn(paneId: string, runtime: string): boolean {
  const entry = entries.get(paneId);
  if (!entry || !switchNow(entry, runtime)) return false;
  entry.runtime = runtime;
  entry.switching = true;
  void killPty(paneId).catch(() => {
    entry.switching = false;
  });
  return true;
}

let focusSuspended = false;

/**
 * While the panes are hidden behind another view, no terminal may take the
 * keyboard: a pane that becomes active then (an agent spawned one) would
 * otherwise pull keystrokes out of whatever the owner is typing into.
 */
export function suspendTerminalFocus(suspended: boolean): void {
  focusSuspended = suspended;
  if (suspended) for (const entry of entries.values()) entry.term.blur();
}

/** Gives the pane's terminal keyboard focus. */
export function focusTerminal(paneId: string): void {
  if (!focusSuspended) entries.get(paneId)?.term.focus();
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
  // A switch under way must not start a shell for a pane that is gone.
  entry.switching = false;
  if (!entry.dead) void killPty(paneId).catch(() => {});
  entry.term.dispose();
  entry.element.remove();
  entries.delete(paneId);
}

/** How long a hand-off waits for output already on its way to this window. */
const HANDOFF_WAIT_MS = 3000;

/**
 * Gives the pane's terminal up to another window: its output is held, this
 * window waits for everything it was already sent, and its screen and
 * scrollback are returned as text-with-escapes for the other window to draw.
 * The terminal stays here, parked, for the pane's return.
 */
export async function handOff(paneId: string): Promise<string> {
  const entry = entries.get(paneId);
  if (!entry) return "";
  let owed = 0;
  if (!entry.dead) owed = await holdPty(paneId).catch(() => 0);
  const until = Date.now() + HANDOFF_WAIT_MS;
  while (entry.received < owed && Date.now() < until) await new Promise((r) => window.setTimeout(r, 10));
  await new Promise<void>((resolve) => entry.term.write("", resolve));
  flushAck(entry);
  entry.away = true;
  detachTerminal(paneId);
  return entry.serialize.serialize();
}

/**
 * Takes a pane's terminal from another window: draws `content` - what that
 * window showed - and receives the pane's output from here on, starting with
 * whatever was held while it moved. `content` null keeps this window's own
 * screen, for a pane whose other window went without handing anything back.
 */
// No runtime here: a taken-over pane's shell is already running (`spawned` is
// set below), so nothing ever starts one from this path. Anything that did
// would need the pane's runtime, or it would start the shell on Windows.
export async function takeOver(paneId: string, content: string | null, cwd?: string, workspaceId?: string): Promise<void> {
  openTerminal(paneId, cwd, workspaceId);
  const entry = entries.get(paneId);
  if (!entry) return;
  if (content !== null) {
    entry.term.reset();
    entry.term.write(content);
  } else if (entry.away) {
    entry.term.write("\r\n\x1b[2m[back from its own window: what it showed there is not repeated here]\x1b[0m\r\n");
  }
  entry.spawned = true;
  entry.away = false;
  entry.received = 0;
  entry.resizeOnShow = true;
  await attachPty(paneId, (bytes) => writeOutput(entry, bytes), (event) => handleEvent(entry, event)).catch(() => {
    entry.dead = true;
  });
}

/** The pane's visible buffer as text-with-escapes (backs `dex pane capture`). */
export function captureTerminal(paneId: string): string | undefined {
  return entries.get(paneId)?.serialize.serialize();
}

/**
 * What is on the pane's screen right now, as plain text, one printed line per
 * line, wrapped rows rejoined. Reads the buffer rather than serializing it:
 * this is polled, and a serialize walks the whole scrollback to produce escapes
 * nobody here wants. From `baseY`, not `viewportY`: what the program last drew,
 * wherever the owner has scrolled to.
 */
export function readScreen(paneId: string): string | undefined {
  const term = entries.get(paneId)?.term;
  if (!term) return undefined;
  const buffer = term.buffer.active;
  const end = buffer.baseY + term.rows;
  const rows: BufferRow[] = [];
  for (let row = buffer.baseY; row < end; row += 1) {
    const line = buffer.getLine(row);
    // Untrimmed: where a wrap fell on a space, the space is part of the line.
    rows.push({ text: line?.translateToString(false) ?? "", wrapped: line?.isWrapped ?? false });
  }
  return joinWrapped(rows).join("\n");
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
  if (entry.dead || entry.away) return;
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
  entry.received += bytes.length;
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
    if (entry.spawned && !entry.dead && !entry.away) void resizePty(entry.paneId, cols, rows).catch(() => {});
  }, RESIZE_DEBOUNCE_MS);
}

function handleEvent(entry: Entry, event: PtyEvent): void {
  if (event.type === "dropped") {
    // Bytes were discarded mid-stream, so the terminal state is unknown: reset it (PRD §7.1).
    entry.term.reset();
    entry.term.write("\x1b[2m[… output dropped while the display was unresponsive …]\x1b[0m\r\n");
    return;
  }
  // Closed to start again in another terminal (`restartIn`): no exit to report.
  if (entry.switching && entries.get(entry.paneId) === entry) {
    entry.switching = false;
    entry.term.write(`\r\n\x1b[2m[now in ${runtimeLabel(entry.runtime ?? "windows")}]\x1b[0m\r\n`);
    startShell(entry);
    return;
  }
  // Keep the pane and its scrollback: output after a crash is often exactly what the user wants.
  entry.dead = true;
  const code = event.code === null ? "unknown" : String(event.code);
  entry.term.write(`\r\n\x1b[2m[process exited with code ${code}]\x1b[0m\r\n`);
}
