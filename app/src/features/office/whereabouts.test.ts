import { beforeEach, describe, expect, it } from "vitest";
import { forgetWhereabouts, noteHome, noteOut, takeWhereabouts } from "./whereabouts";

const spot = { x: 110, y: 578, corridor: 400 };

describe("whereabouts", () => {
  beforeEach(() => forgetWhereabouts());

  it("knows where someone out of their cubicle was last seen", () => {
    noteOut("a", spot);
    expect(takeWhereabouts("a")).toEqual(spot);
  });

  it("is asked once: whoever walks out from there is then gone", () => {
    noteOut("a", spot);
    takeWhereabouts("a");
    expect(takeWhereabouts("a")).toBeNull();
  });

  it("knows nothing of someone at their desk", () => {
    noteOut("a", spot);
    noteHome("a");
    expect(takeWhereabouts("a")).toBeNull();
    expect(takeWhereabouts("never-seen")).toBeNull();
  });

  it("keeps the latest place", () => {
    noteOut("a", spot);
    noteOut("a", { x: 730, y: 400, corridor: 400 });
    expect(takeWhereabouts("a")).toEqual({ x: 730, y: 400, corridor: 400 });
  });
});
