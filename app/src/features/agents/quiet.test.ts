import { describe, expect, it } from "vitest";
import { quietSpan, quietWords } from "./quiet";

describe("quietSpan", () => {
  it("counts seconds, then minutes, then hours and minutes", () => {
    expect(quietSpan(40_000)).toBe("40s");
    expect(quietSpan(12 * 60_000)).toBe("12m");
    expect(quietSpan(63 * 60_000)).toBe("1h 3m");
  });

  it("never reads as negative, whatever the clocks say", () => {
    expect(quietSpan(-5_000)).toBe("0s");
  });
});

describe("quietWords", () => {
  it("says how long the agent has been quiet", () => {
    expect(quietWords(12 * 60_000)).toBe("nothing for 12m");
  });

  it("says nothing when the daemon sent nothing", () => {
    // Every status but `running`, and silences short enough to be work.
    expect(quietWords(null)).toBeNull();
    expect(quietWords(undefined)).toBeNull();
  });

  it("says something for a silence of zero, which is a threshold of zero", () => {
    expect(quietWords(0)).toBe("nothing for 0s");
  });
});
