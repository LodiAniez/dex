import { describe, expect, it } from "vitest";
import { askedYou, lastSaid, seenAs } from "./attention";

describe("askedYou", () => {
  it("is what an idle agent asked as its turn ended", () => {
    expect(askedYou({ status: "idle", status_detail: 'asked you: Reply "yes" and I\'ll overwrite the file.' })).toBe('Reply "yes" and I\'ll overwrite the file.');
  });

  it("is nothing for an agent that simply finished", () => {
    expect(askedYou({ status: "idle", status_detail: null })).toBeNull();
  });

  it("is what an agent asked with Claude Code's question dialog, where it is truly waiting", () => {
    expect(askedYou({ status: "waiting", status_detail: "asked you: Do you prefer red or blue?" })).toBe("Do you prefer red or blue?");
  });

  it("is nothing for any other status, whatever its detail says", () => {
    expect(askedYou({ status: "waiting", status_detail: "permission to Edit a.rs" })).toBeNull();
    expect(askedYou({ status: "running", status_detail: "asked you: stale" })).toBeNull();
    expect(askedYou({ status: "error", status_detail: "rate_limit" })).toBeNull();
  });
});

describe("askedYou and what was merely said", () => {
  it("is not fooled by an agent that finished and said so", () => {
    const finished = { status: "idle" as const, status_detail: "said: All 14 tests pass. Nothing else was touched." };
    expect(askedYou(finished)).toBeNull();
    expect(seenAs(finished)).toBe("idle");
  });
});

describe("lastSaid", () => {
  it("is how an idle agent's turn ended, question or not, for the owner to read", () => {
    expect(lastSaid({ status: "idle", status_detail: "said: All 14 tests pass." })).toBe("All 14 tests pass.");
    expect(lastSaid({ status: "idle", status_detail: "asked you: Should I run it?" })).toBe("Should I run it?");
  });

  it("is nothing for an agent that is working, waiting at a dialog, or has said nothing", () => {
    expect(lastSaid({ status: "idle", status_detail: null })).toBeNull();
    expect(lastSaid({ status: "running", status_detail: "said: old news" })).toBeNull();
    expect(lastSaid({ status: "waiting", status_detail: "permission to Edit a.rs" })).toBeNull();
  });
});

describe("seenAs", () => {
  it("shows an agent that asked the owner something as waiting for them, though Claude Code calls it idle", () => {
    expect(seenAs({ status: "idle", status_detail: "asked you: Should I run it?" })).toBe("waiting");
  });

  it("shows everyone else as they are", () => {
    expect(seenAs({ status: "idle", status_detail: null })).toBe("idle");
    for (const status of ["running", "waiting", "error", "unknown", "dead"] as const) {
      expect(seenAs({ status, status_detail: "anything" })).toBe(status);
    }
  });
});
