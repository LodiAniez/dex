import { describe, expect, it } from "vitest";
import { DRAG_START_PX, dragKey, dropSide, isDrag } from "./paneDrop";

const box = { left: 100, top: 200, width: 400, height: 200 };
const at = (fx: number, fy: number) => ({ x: box.left + fx * box.width, y: box.top + fy * box.height });

describe("dropSide", () => {
  it("is the middle of a pane: trade places", () => {
    expect(dropSide(box, at(0.5, 0.5))).toBe("center");
    expect(dropSide(box, at(0.35, 0.6))).toBe("center");
  });

  it("is the edge the pointer is nearest, within a quarter of the pane", () => {
    expect(dropSide(box, at(0.05, 0.5))).toBe("left");
    expect(dropSide(box, at(0.95, 0.5))).toBe("right");
    expect(dropSide(box, at(0.5, 0.1))).toBe("top");
    expect(dropSide(box, at(0.5, 0.9))).toBe("bottom");
  });

  it("picks the nearer edge in a corner", () => {
    expect(dropSide(box, at(0.05, 0.2))).toBe("left");
    expect(dropSide(box, at(0.2, 0.05))).toBe("top");
  });

  it("is nothing outside the pane", () => {
    expect(dropSide(box, { x: 50, y: 250 })).toBeNull();
    expect(dropSide(box, { x: 300, y: 450 })).toBeNull();
  });
});

describe("isDrag", () => {
  it("is not a click: the pointer has to travel before a pane lifts", () => {
    expect(isDrag({ x: 10, y: 10 }, { x: 12, y: 11 })).toBe(false);
    expect(isDrag({ x: 10, y: 10 }, { x: 10 + DRAG_START_PX, y: 10 })).toBe(true);
    expect(isDrag({ x: 10, y: 10 }, { x: 10, y: 10 - DRAG_START_PX - 1 })).toBe(true);
  });
});

describe("dragKey", () => {
  it("cancels a lifted pane on Escape, and keeps the key from the terminal: Escape interrupts a Claude Code turn", () => {
    expect(dragKey(true, "Escape")).toEqual({ cancel: true, swallow: true });
  });

  it("swallows every other key while a pane is lifted, so nothing is typed into a pane mid-drag", () => {
    expect(dragKey(true, "a")).toEqual({ cancel: false, swallow: true });
  });

  it("leaves the keyboard alone before the pane has lifted: a press on a header is still just a click", () => {
    expect(dragKey(false, "Escape")).toEqual({ cancel: true, swallow: false });
    expect(dragKey(false, "a")).toEqual({ cancel: false, swallow: false });
  });
});
