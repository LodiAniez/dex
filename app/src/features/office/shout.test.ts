import { describe, expect, it } from "vitest";
import { BURST_SECONDS, HEAD_START_SECONDS, SHOUT_SECONDS, hear, holdsTheDoor, isShouting, newHires, shoutText } from "./shout";

const agent = (id: string, parent_id: string | null = null) => ({ id, parent_id });

describe("shoutText", () => {
  it("asks HR for as many engineers as were sent for", () => {
    expect(shoutText(3)).toBe("I need 3 engineers on the floor ASAP!");
    expect(shoutText(10)).toBe("I need 10 engineers on the floor ASAP!");
  });

  it("asks for one engineer, not one engineers", () => {
    expect(shoutText(1)).toBe("I need 1 engineer on the floor ASAP!");
  });
});

describe("newHires", () => {
  it("is whoever another agent has just sent for, with who sent for them", () => {
    const known = new Set(["lead"]);
    expect(newHires(known, [agent("lead"), agent("a", "lead"), agent("b", "lead")])).toEqual([
      { id: "a", hirer: "lead" },
      { id: "b", hirer: "lead" },
    ]);
  });

  it("is nobody the owner hired: HR did that, and nobody shouted", () => {
    expect(newHires(new Set(["lead"]), [agent("lead"), agent("mine")])).toEqual([]);
  });

  it("is nobody already seen", () => {
    expect(newHires(new Set(["lead", "a"]), [agent("lead"), agent("a", "lead")])).toEqual([]);
  });

  it("is nobody when the map has only just opened: they were sent for before anyone was watching", () => {
    expect(newHires(null, [agent("lead"), agent("a", "lead")])).toEqual([]);
  });
});

describe("hear", () => {
  it("starts a shout for one", () => {
    expect(hear(null, "lead", 1000)).toEqual({ hirer: "lead", count: 1, heardAt: 1000 });
  });

  it("grows while the same agent keeps sending for more, and is shouted afresh", () => {
    const first = hear(null, "lead", 1000);
    const second = hear(first, "lead", 1000 + 5000);
    expect(second).toEqual({ hirer: "lead", count: 2, heardAt: 6000 });
    expect(hear(second, "lead", 6000 + BURST_SECONDS * 1000)).toMatchObject({ count: 3 });
  });

  it("starts again after a lull: that is another round of hiring", () => {
    const first = hear(null, "lead", 1000);
    expect(hear(first, "lead", 1000 + BURST_SECONDS * 1000 + 1)).toEqual({ hirer: "lead", count: 1, heardAt: 1000 + BURST_SECONDS * 1000 + 1 });
  });

  it("starts again for someone else", () => {
    expect(hear(hear(null, "lead", 1000), "other", 1500)).toEqual({ hirer: "other", count: 1, heardAt: 1500 });
  });
});

describe("isShouting", () => {
  it("lasts a few seconds from the last time it was heard", () => {
    const shout = hear(null, "lead", 1000);
    expect(isShouting(shout, 1000)).toBe(true);
    expect(isShouting(shout, 1000 + SHOUT_SECONDS * 1000 - 1)).toBe(true);
    expect(isShouting(shout, 1000 + SHOUT_SECONDS * 1000)).toBe(false);
    expect(isShouting(null, 1000)).toBe(false);
  });
});

describe("holdsTheDoor", () => {
  it("keeps a hire inside HR until the shout has had a moment: cause before effect", () => {
    const shout = hear(null, "lead", 1000);
    expect(holdsTheDoor(shout, 1000)).toBe(true);
    expect(holdsTheDoor(shout, 1000 + HEAD_START_SECONDS * 1000)).toBe(false);
  });

  it("holds nobody when nobody has shouted", () => {
    expect(holdsTheDoor(null, 1000)).toBe(false);
  });

  it("is shorter than the shout, so the hire is seen answering it", () => {
    expect(HEAD_START_SECONDS).toBeLessThan(SHOUT_SECONDS);
  });
});
