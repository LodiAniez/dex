import { describe, expect, it } from "vitest";
import type { MirrorMessage } from "./mirrorFeed";
import { MirrorLink, type Wire } from "./mirrorLink";

/** A pane that can be here or elsewhere, and a record of what the link did. */
function world() {
  const log: string[] = [];
  const sent: MirrorMessage[] = [];
  let paneHere = false;
  let feeds = 0;
  let toPane: ((message: MirrorMessage) => void) | null = null;
  let next = 0;
  const link = new MirrorLink(
    {
      here(send) {
        if (!paneHere) return null;
        const n = ++feeds;
        toPane = send;
        log.push(`here ${n}`);
        return () => log.push(`unhere ${n}`);
      },
      isHere: () => paneHere,
      ask: (id) => log.push(`ask ${id}`),
      cancel: (id) => log.push(`cancel ${id}`),
      newId: () => `r${++next}`,
    },
    (message) => sent.push(message),
  );
  return {
    link,
    log,
    sent,
    setHere: (here: boolean) => (paneHere = here),
    fromPane: (message: MirrorMessage) => toPane?.(message),
  };
}

const screen: MirrorMessage = { kind: "screen", content: "S", cols: 80, rows: 24 };
const hello: Wire = { kind: "hello" };

describe("a mirror link to a pane in this window", () => {
  it("watches it here, and passes on what it prints", () => {
    const w = world();
    w.setHere(true);
    w.link.start();
    w.fromPane(screen);
    expect(w.log).toEqual(["here 1"]);
    expect(w.sent).toEqual([screen]);
  });

  it("looks again when the pane leaves, and asks the window it went to", () => {
    const w = world();
    w.setHere(true);
    w.link.start();
    w.link.ready();
    w.setHere(false);
    w.fromPane({ kind: "end" });
    expect(w.sent).toEqual([{ kind: "end" }]);
    expect(w.log).toEqual(["here 1", "unhere 1", "ask r1"]);
  });

  it("does not start watching again when the pane is closed while it is told", () => {
    // Closing removes the pane before its mirrors hear of it: the link finds
    // nothing here, so it cannot keep re-attaching to a pane being closed.
    const w = world();
    w.setHere(true);
    w.link.start();
    w.setHere(false);
    w.fromPane({ kind: "end" });
    expect(w.log.filter((line) => line.startsWith("here"))).toHaveLength(1);
  });
});

describe("a mirror link to a pane in another window", () => {
  it("asks only once it can hear the answer", () => {
    const w = world();
    w.link.start();
    expect(w.log).toEqual([]);
    w.link.ready();
    expect(w.log).toEqual(["ask r1"]);
  });

  it("takes the answer to its question, and no other", () => {
    const w = world();
    w.link.start();
    w.link.ready();
    w.link.heard("r0", screen);
    w.link.heard("r1", hello);
    w.link.heard("r1", screen);
    expect(w.sent).toEqual([screen]);
  });

  it("asks again when nobody said hello, and lets go of the old question", () => {
    const w = world();
    w.link.start();
    w.link.ready();
    w.link.tick();
    expect(w.log).toEqual(["ask r1", "cancel r1", "ask r2"]);
  });

  it("waits, once greeted, however long the screen takes", () => {
    // A minimised window draws late; asking again would cancel it every time.
    const w = world();
    w.link.start();
    w.link.ready();
    w.link.heard("r1", hello);
    w.link.tick();
    w.link.tick();
    expect(w.log).toEqual(["ask r1"]);
  });

  it("watches the pane here once it comes back without a word", () => {
    const w = world();
    w.link.start();
    w.link.ready();
    w.link.heard("r1", hello);
    w.setHere(true);
    w.link.tick();
    expect(w.log).toEqual(["ask r1", "cancel r1", "here 1"]);
  });

  it("sends nothing after it is stopped, and lets go of its question", () => {
    const w = world();
    w.link.start();
    w.link.ready();
    w.link.stop();
    w.link.heard("r1", screen);
    w.link.tick();
    expect(w.sent).toEqual([]);
    expect(w.log).toEqual(["ask r1", "cancel r1"]);
  });
});
