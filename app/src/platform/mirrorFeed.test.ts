import { describe, expect, it } from "vitest";
import { MirrorFeed, type MirrorMessage } from "./mirrorFeed";

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
