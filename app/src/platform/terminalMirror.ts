/**
 * A live, read-only copy of a pane's terminal, for the office's monitor: the
 * screen as it is, then everything the pane prints (`mirrorFeed.ts`). The pane
 * is only watched - its terminal is not moved, resized or written to - so it
 * carries on unchanged in the terminal view, or in a window of its own.
 *
 * A pane in a window of its own gets its output there, not here, so that window
 * is asked for the copy over app events; it answers for its own pane only.
 */
import { emit, listen } from "@tauri-apps/api/event";
import { MirrorFeed, type MirrorMessage } from "./mirrorFeed";
import { entries } from "./terminalEntries";

/** How much history the copy starts with: the screen and a little above it. */
const SCROLLBACK = 200;

const START = "mirror-start";
const STOP = "mirror-stop";
const MESSAGE = "mirror-message";

interface Start {
  id: string;
  paneId: string;
}
interface Carried {
  id: string;
  message: MirrorMessage;
}

/** Mirrors a pane whose terminal this window holds; null if it does not hold it. */
function mirrorHere(paneId: string, send: (message: MirrorMessage) => void): (() => void) | null {
  const entry = entries.get(paneId);
  if (!entry || entry.away) return null;
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
 * called: from this window's terminal, or from the pane's own window.
 */
export function mirrorTerminal(paneId: string, send: (message: MirrorMessage) => void): () => void {
  const local = mirrorHere(paneId, send);
  if (local) return local;
  const id = crypto.randomUUID();
  let stopped = false;
  let unlisten: (() => void) | null = null;
  void listen<Carried>(MESSAGE, (event) => {
    if (!stopped && event.payload.id === id) send(event.payload.message);
  }).then((stop) => {
    if (stopped) {
      stop();
      return;
    }
    unlisten = stop;
    void emit(START, { id, paneId } satisfies Start);
  });
  return () => {
    stopped = true;
    unlisten?.();
    void emit(STOP, { id }).catch(() => {});
  };
}

/** In a pane's own window: answers requests to mirror the pane it holds. */
export async function serveMirrors(): Promise<() => void> {
  const running = new Map<string, () => void>();
  const starting = await listen<Start>(START, (event) => {
    const { id, paneId } = event.payload;
    const stop = mirrorHere(paneId, (message) => void emit(MESSAGE, { id, message } satisfies Carried));
    if (stop) running.set(id, stop);
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
