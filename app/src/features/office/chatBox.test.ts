import { describe, expect, it } from "vitest";
import { caughtUp, isMinimized, newest, storedAs, unseen, unseenLabel } from "./chatBox";

const line = (seq: number) => ({ seq });

describe("isMinimized", () => {
  it("is open until the owner has minimized it", () => {
    expect(isMinimized(null)).toBe(false);
    expect(isMinimized("open")).toBe(false);
    expect(isMinimized("rubbish")).toBe(false);
  });

  it("reads back what was stored", () => {
    expect(isMinimized(storedAs(true))).toBe(true);
    expect(isMinimized(storedAs(false))).toBe(false);
  });
});

describe("newest", () => {
  it("is the latest line's number, and 0 for a silent office", () => {
    expect(newest([line(4), line(9), line(7)])).toBe(9);
    expect(newest([])).toBe(0);
  });
});

describe("unseen", () => {
  it("counts what has happened since the box was minimized", () => {
    expect(unseen([line(4), line(7), line(9)], 4)).toBe(2);
    expect(unseen([line(4), line(7), line(9)], 9)).toBe(0);
  });

  it("counts nothing for a box minimized in an earlier session: nobody knows what was seen", () => {
    expect(unseen([line(4), line(7)], null)).toBe(0);
  });
});

describe("caughtUp", () => {
  it("takes what is there when a minimized box is first shown as seen, so that what comes after is counted", () => {
    const seen = caughtUp(null, [line(4), line(7)]);
    expect(seen).toBe(7);
    expect(unseen([line(4), line(7), line(9)], seen)).toBe(1);
  });

  it("waits for the first line in a silent office", () => {
    expect(caughtUp(null, [])).toBeNull();
  });

  it("leaves alone what is already known", () => {
    expect(caughtUp(4, [line(4), line(7)])).toBe(4);
  });
});

describe("unseenLabel", () => {
  it("says nothing when there is nothing new", () => {
    expect(unseenLabel(0, 4)).toBeNull();
  });

  it("is the count", () => {
    expect(unseenLabel(2, 4)).toBe("2");
  });

  it("admits it may be more once every line the box holds is new", () => {
    expect(unseenLabel(4, 4)).toBe("4+");
  });
});
