/**
 * A live, read-only copy of a pane's terminal, for the office's monitor: the
 * screen as it is, then everything the pane prints (`mirrorFeed.ts`). The pane
 * is only watched - its terminal is not moved, resized or written to - so it
 * carries on unchanged in the terminal view, or in a window of its own.
 *
 * A pane in a window of its own gets its output there, not here, so that window
 * is asked for the copy over app events and answers the window that asked. The
 * copy follows the pane: when it moves between windows, the copy starts again
 * from wherever it is now, with a fresh screen.
 */
import { emit, emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MirrorFeed, type MirrorMessage } from "./mirrorFeed";
import { entries } from "./terminalEntries";

/** How much history the copy starts with: the screen and a little above it. */
const SCROLLBACK = 200;
/** How often a copy checks where the pane is, and asks again if nobody answered. */
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
  message: MirrorMessage;
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
 * gets a fresh `screen` whenever the copy starts again.
 */
export function mirrorTerminal(paneId: string, send: (message: MirrorMessage) => void): () => void {
  const me = getCurrentWindow().label;
  let stopped = false;
  let stopLocal: (() => void) | null = null;
  let asked: string | null = null;
  let answered = false;

  const forget = () => {
    stopLocal?.();
    stopLocal = null;
    if (asked !== null) quietly(emit(STOP, { id: asked }));
    asked = null;
  };
  const receive = (message: MirrorMessage) => {
    // The pane moved or closed: look for it again.
    if (message.kind === "end") attach();
    else send(message);
  };
  const attach = () => {
    forget();
    if (stopped) return;
    stopLocal = mirrorHere(paneId, receive);
    if (stopLocal) return;
    asked = crypto.randomUUID();
    answered = false;
    quietly(emit(START, { id: asked, paneId, to: me } satisfies Start));
  };

  let unlisten: (() => void) | null = null;
  listen<Carried>(MESSAGE, (event) => {
    if (stopped || asked === null || event.payload.id !== asked) return;
    answered = true;
    receive(event.payload.message);
  }).then(
    (stop) => (stopped ? stop() : (unlisten = stop)),
    () => {},
  );
  // Asked before the pane's window was listening, or the pane came back here
  // without a word (its window closed): look again.
  const timer = window.setInterval(() => {
    const entry = entries.get(paneId);
    const hereNow = entry !== undefined && !entry.away;
    if ((stopLocal === null && hereNow) || (asked !== null && !answered)) attach();
  }, RETRY_MS);

  attach();
  return () => {
    stopped = true;
    window.clearInterval(timer);
    forget();
    unlisten?.();
  };
}

/** In a pane's own window: answers requests to mirror the pane it holds. */
export async function serveMirrors(): Promise<() => void> {
  const running = new Map<string, () => void>();
  const starting = await listen<Start>(START, (event) => {
    const { id, paneId, to } = event.payload;
    if (running.has(id)) return;
    const deliver = (message: MirrorMessage) => quietly(emitTo(to, MESSAGE, { id, message } satisfies Carried));
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
    if (stop) {
      running.set(id, () => {
        flush();
        stop();
      });
    }
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
