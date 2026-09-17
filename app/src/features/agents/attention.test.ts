import { describe, expect, it } from "vitest";
import { askedYou, seenAs } from "./attention";

describe("askedYou", () => {
  it("is what an idle agent asked as its turn ended", () => {
    expect(askedYou({ status: "idle", status_detail: 'asked you: Reply "yes" and I\'ll overwrite the file.' })).toBe('Reply "yes" and I\'ll overwrite the file.');
  });

  it("is nothing for an agent that simply finished", () => {
    expect(askedYou({ status: "idle", status_detail: null })).toBeNull();
  });

  it("is nothing for any other status, whatever its detail says", () => {
    expect(askedYou({ status: "waiting", status_detail: "permission to Edit a.rs" })).toBeNull();
    expect(askedYou({ status: "error", status_detail: "rate_limit" })).toBeNull();
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
