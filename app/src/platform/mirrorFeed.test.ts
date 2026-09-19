import { describe, expect, it } from "vitest";
import { MirrorFeed, endAll, type MirrorMessage } from "./mirrorFeed";

function feed() {
  const sent: MirrorMessage[] = [];
  return { sent, feed: new MirrorFeed((message) => sent.push(message)) };
}

describe("a mirror feed", () => {
  it("starts with the screen as it was, then everything printed after it", () => {
    const { sent, feed: f } = feed();
    f.start("SCREEN", 120, 30);
    f.data("a");
    f.data("b");
    expect(sent).toEqual([
      { kind: "screen", content: "SCREEN", cols: 120, rows: 30 },
      { kind: "data", text: "a" },
      { kind: "data", text: "b" },
    ]);
  });

  it("keeps what is printed while the screen is being read, and sends it after, in order", () => {
    const { sent, feed: f } = feed();
    f.data("printed while reading ");
    f.data("the screen");
    expect(sent).toEqual([]);
    f.start("SCREEN", 80, 24);
    expect(sent).toEqual([
      { kind: "screen", content: "SCREEN", cols: 80, rows: 24 },
      { kind: "data", text: "printed while reading the screen" },
    ]);
  });

  it("passes on a resize in its place among the output", () => {
    const { sent, feed: f } = feed();
    f.data("x");
    f.resize(100, 40);
    f.data("y");
    f.start("S", 80, 24);
    expect(sent).toEqual([
      { kind: "screen", content: "S", cols: 80, rows: 24 },
      { kind: "data", text: "x" },
      { kind: "resize", cols: 100, rows: 40 },
      { kind: "data", text: "y" },
    ]);
  });

  it("sends nothing once stopped", () => {
    const { sent, feed: f } = feed();
    f.start("S", 80, 24);
    f.stop();
    f.data("late");
    f.resize(10, 10);
    expect(sent).toHaveLength(1);
  });

  it("drops the backlog rather than grow without end if the screen never comes", () => {
    const { sent, feed: f } = feed();
    for (let i = 0; i < 5000; i++) f.data("0123456789");
    f.start("S", 80, 24);
    const backlog = sent.slice(1).map((m) => (m.kind === "data" ? m.text : "")).join("");
    expect(backlog.length).toBeLessThanOrEqual(MirrorFeed.MAX_BACKLOG);
    // The newest output is what is kept.
    expect(backlog.endsWith("0123456789")).toBe(true);
  });
});

describe("a mirror feed ending", () => {
  it("tells the mirror it ended, once, and sends nothing after", () => {
    const { sent, feed: f } = feed();
    f.start("S", 80, 24);
    f.end();
    f.end();
    f.data("late");
    expect(sent).toEqual([{ kind: "screen", content: "S", cols: 80, rows: 24 }, { kind: "end" }]);
  });

  it("says so even before the screen came", () => {
    const { sent, feed: f } = feed();
    f.data("x");
    f.end();
    expect(sent).toEqual([{ kind: "end" }]);
  });
});

describe("a mirror feed's backlog", () => {
  it("keeps the latest size when old output is dropped, so the mirror lays out as the pane does", () => {
    const { sent, feed: f } = feed();
    f.resize(100, 40);
    for (let i = 0; i < 30000; i++) f.data("0123456789");
    f.start("S", 80, 24);
    expect(sent[1]).toEqual({ kind: "resize", cols: 100, rows: 40 });
  });
});

describe("ending every mirror of a pane", () => {
  it("ends each once and empties the set, even when one starts watching again as it is told", () => {
    const mirrors = new Set<MirrorFeed>();
    let ended = 0;
    const add = () => {
      const f = new MirrorFeed((message) => {
        if (message.kind !== "end") return;
        ended += 1;
        // Told the pane is going, it looks again - and finds the same set.
        if (ended < 100) add();
      });
      mirrors.add(f);
    };
    add();
    add();
    endAll(mirrors);
    expect(ended).toBe(2);
    // What started watching during the ending is left in the set, not looped over.
    expect(mirrors.size).toBe(2);
  });
});
