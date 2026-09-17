import { describe, expect, it } from "vitest";
import { MAP_WIDTH, POD, floorLines, mapHeight, podOrigin } from "./mapGeometry";

describe("podOrigin", () => {
  it("puts the first six pods where the design drew them", () => {
    expect([0, 1, 2, 3, 4, 5].map(podOrigin)).toEqual([
      { x: 340, y: 130 },
      { x: 650, y: 130 },
      { x: 960, y: 130 },
      { x: 340, y: 480 },
      { x: 650, y: 480 },
      { x: 960, y: 480 },
    ]);
  });

  it("carries on downwards, a row at a time", () => {
    expect(podOrigin(6)).toEqual({ x: 340, y: 830 });
    expect(podOrigin(11)).toEqual({ x: 960, y: 1180 });
  });

  it("never lets two pods touch, or one leave the building", () => {
    for (let pod = 0; pod < 12; pod += 1) {
      const { x, y } = podOrigin(pod);
      expect(x + POD.width).toBeLessThan(MAP_WIDTH - 14);
      expect(y + POD.height).toBeLessThan(mapHeight(12) - 14);
      if (pod % 3 !== 0) expect(x).toBeGreaterThanOrEqual(podOrigin(pod - 1).x + POD.width);
      if (pod >= 3) expect(y).toBeGreaterThanOrEqual(podOrigin(pod - 3).y + POD.height);
    }
  });
});

describe("mapHeight", () => {
  it("is the design's height for two rows", () => {
    expect(mapHeight(6)).toBe(832);
  });

  it("does not shrink below two rows, where HR and the break room live", () => {
    expect(mapHeight(3)).toBe(832);
  });

  it("grows by a row's height for each row past two", () => {
    expect(mapHeight(9)).toBe(832 + 350);
    expect(mapHeight(7)).toBe(832 + 350);
    expect(mapHeight(12)).toBe(832 + 700);
  });
});

describe("floorLines", () => {
  it("lays the design's planks in the design's building", () => {
    expect(floorLines(832)).toEqual([180, 340, 500, 660]);
  });

  it("keeps laying them to the bottom of a taller one, and stops short of the wall", () => {
    const lines = floorLines(832 + 350);
    expect(lines.slice(0, 4)).toEqual([180, 340, 500, 660]);
    expect(lines.at(-1)).toBeLessThan(832 + 350 - 14);
    expect(lines.length).toBeGreaterThan(4);
  });
});
