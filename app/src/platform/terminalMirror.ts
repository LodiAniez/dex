/**
 * A live, read-only copy of a pane's terminal, for the office's monitor: the
 * screen as it is, then everything the pane prints (`mirrorFeed.ts`). The pane
 * is only watched - its terminal is not moved, resized or written to - so it
 * carries on unchanged in the terminal view, or in a window of its own.
 *
 * A pane in a window of its own gets its output there, not here, so that window
 * is asked for the copy over app events: it says hello at once, then sends the
 * copy, in order, to the window that asked. The copy follows the pane between
 * windows (`mirrorLink.ts`).
 */
import { emit, emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MirrorFeed, type MirrorMessage } from "./mirrorFeed";
import { MirrorLink, type Wire } from "./mirrorLink";
import { entries } from "./terminalEntries";

/** How much history the copy starts with: the screen and a little above it. */
const SCROLLBACK = 200;
/** How often a copy checks where the pane is, and asks again if nobody said hello. */
const RETRY_MS = 1000;
/** Output sent between windows is gathered this long: one event per frame, not per chunk. */
const BATCH_MS = 16;

const START = "mirror-start";
const STOP = "mirror-stop";
const MESSAGE = "mirror-message";

interface Start {
  id: string;
  paneId: string;
  /** The window that asked, and gets the answers. */
  to: string;
}
interface Carried {
  id: string;
  message: Wire;
}

const quietly = (sent: Promise<unknown>) => void sent.catch(() => {});

/** Mirrors a pane whose terminal this window holds; null if it does not hold it. */
function mirrorHere(paneId: string, send: (message: MirrorMessage) => void): (() => void) | null {
  const entry = entries.get(paneId);
  if (!entry || entry.away) return null;
  // A decoder an earlier mirror left mid-character would start this one with garbage.
  if (entry.mirrors.size === 0) entry.decoder = new TextDecoder();
  const feed = new MirrorFeed(send);
  entry.mirrors.add(feed);
  // The screen is read once everything already written has been drawn:
  // output after this point goes to the feed, and is not on the screen read.
  entry.term.write("", () => feed.start(entry.serialize.serialize({ scrollback: SCROLLBACK }), entry.term.cols, entry.term.rows));
  return () => {
    feed.stop();
    entry.mirrors.delete(feed);
  };
}

/**
 * Sends `send` a copy of the pane's terminal until the returned function is
 * called: from this window's terminal, or from the pane's own window. `send`
 * gets a fresh `screen` whenever the copy starts again, after an `end`.
 */
export function mirrorTerminal(paneId: string, send: (message: MirrorMessage) => void): () => void {
  const me = getCurrentWindow().label;
  const link = new MirrorLink(
    {
      here: (to) => mirrorHere(paneId, to),
      isHere: () => {
        const entry = entries.get(paneId);
        return entry !== undefined && !entry.away;
      },
      ask: (id) => quietly(emit(START, { id, paneId, to: me } satisfies Start)),
      cancel: (id) => quietly(emit(STOP, { id })),
      newId: () => crypto.randomUUID(),
    },
    send,
  );
  let stopped = false;
  let unlisten: (() => void) | null = null;
  listen<Carried>(MESSAGE, (event) => link.heard(event.payload.id, event.payload.message)).then(
    (stop) => {
      if (stopped) return stop();
      unlisten = stop;
      link.ready();
    },
    (err) => console.warn("monitor: cannot hear other windows", err),
  );
  const timer = window.setInterval(() => link.tick(), RETRY_MS);
  link.start();
  return () => {
    stopped = true;
    window.clearInterval(timer);
    link.stop();
    unlisten?.();
  };
}

/** In a pane's own window: answers requests to mirror the pane it holds. */
export async function serveMirrors(): Promise<() => void> {
  const running = new Map<string, () => void>();
  const starting = await listen<Start>(START, (event) => {
    const { id, paneId, to } = event.payload;
    if (running.has(id)) return;
    // One after another: each emit is its own async call, and two in flight
    // may land in either order - a size before its screen, an end before the
    // last output.
    let chain: Promise<unknown> = Promise.resolve();
    const deliver = (message: Wire) => {
      chain = chain.then(() => emitTo(to, MESSAGE, { id, message } satisfies Carried)).catch(() => {});
    };
    let gathered = "";
    let timer: number | null = null;
    const flush = () => {
      if (timer !== null) window.clearTimeout(timer);
      timer = null;
      if (gathered) deliver({ kind: "data", text: gathered });
      gathered = "";
    };
    const stop = mirrorHere(paneId, (message) => {
      if (message.kind === "data") {
        gathered += message.text;
        timer ??= window.setTimeout(flush, BATCH_MS);
        return;
      }
      flush();
      deliver(message);
    });
    if (!stop) return;
    // At once, before the screen: the asker waits for that, however slow.
    deliver({ kind: "hello" });
    running.set(id, () => {
      flush();
      stop();
    });
  });
  const stopping = await listen<{ id: string }>(STOP, (event) => {
    running.get(event.payload.id)?.();
    running.delete(event.payload.id);
  });
  return () => {
    starting();
    stopping();
    for (const stop of running.values()) stop();
    running.clear();
  };
}
