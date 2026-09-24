import { describe, expect, it } from "vitest";
import { quietSince, quietSpan } from "./quiet";

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

describe("quietSince", () => {
  it("says nothing unless the daemon called the silence worth mentioning", () => {
    // Every status but `running`, and silences short enough to be work.
    expect(quietSince({ quiet_for_ms: null, last_event_at: 0 }, 60 * 60_000)).toBeNull();
  });

  it("goes on counting from the last hook, not from what the daemon last said", () => {
    const agent = { quiet_for_ms: 5 * 60_000, last_event_at: 1_000 };
    // Two minutes after that listing: the line has moved on with the clock.
    expect(quietSince(agent, 1_000 + 7 * 60_000)).toBe("7m");
  });

  it("never falls below what the daemon said, however the clocks disagree", () => {
    const agent = { quiet_for_ms: 6 * 60_000, last_event_at: 10 * 60_000 };
    expect(quietSince(agent, 11 * 60_000)).toBe("6m");
  });
});
