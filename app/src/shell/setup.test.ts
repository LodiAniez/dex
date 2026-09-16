import { describe, expect, it } from "vitest";
import { type Check, type DoctorReport, failures, fingerprint, headline, shouldOffer, stepFor } from "./setup";

function report(...checks: Check[]): DoctorReport {
  return { ok: checks.every((c) => c.status !== "fail"), checks };
}

const ok = (name: string): Check => ({ name, status: "ok", detail: "" });
const fail = (name: string, detail = ""): Check => ({ name, status: "fail", detail });
const skip = (name: string): Check => ({ name, status: "skip", detail: "" });

describe("which failures have a button", () => {
  it("offers to install hooks and register the MCP server", () => {
    expect(stepFor(fail("hooks"))).toBe("hooks");
    expect(stepFor(fail("mcp"))).toBe("mcp");
  });

  it("only explains what it cannot fix", () => {
    // Installing Claude Code or git is the owner's to do.
    expect(stepFor(fail("claude"))).toBeNull();
    expect(stepFor(fail("git"))).toBeNull();
    expect(stepFor(fail("version"))).toBeNull();
  });

  it("offers nothing for a check that passed or was skipped", () => {
    expect(stepFor(ok("hooks"))).toBeNull();
    expect(stepFor(skip("mcp"))).toBeNull();
  });
});

describe("when to open the panel unasked", () => {
  it("never, when everything passes", () => {
    expect(shouldOffer(report(ok("app"), ok("hooks")), null)).toBe(false);
  });

  it("when something fails and nothing was dismissed", () => {
    expect(shouldOffer(report(ok("app"), fail("hooks")), null)).toBe(true);
  });

  it("not again for the same problems the owner dismissed", () => {
    const r = report(fail("hooks"), fail("mcp"));
    expect(shouldOffer(r, fingerprint(r))).toBe(false);
  });

  it("again when a different problem appears", () => {
    // Dismissing "hooks missing" today must not hide "git missing" tomorrow.
    const before = report(fail("hooks"));
    const after = report(fail("hooks"), fail("git"));
    expect(shouldOffer(after, fingerprint(before))).toBe(true);
  });

  it("does not count a skipped check as a problem", () => {
    expect(shouldOffer(report(skip("version")), null)).toBe(false);
  });
});

describe("the fingerprint", () => {
  it("does not depend on the order doctor listed the checks", () => {
    expect(fingerprint(report(fail("mcp"), fail("hooks")))).toBe(fingerprint(report(fail("hooks"), fail("mcp"))));
  });

  it("is empty when nothing fails", () => {
    expect(fingerprint(report(ok("a"), skip("b")))).toBe("");
  });
});

describe("the headline", () => {
  it("counts steps when every failure has a button", () => {
    expect(headline(report(fail("hooks")))).toBe("One step left to set up Dex");
    expect(headline(report(fail("hooks"), fail("mcp")))).toBe("2 steps left to set up Dex");
  });

  it("says attention when something needs the owner", () => {
    expect(headline(report(fail("claude")))).toBe("One thing needs your attention");
    expect(headline(report(fail("claude"), fail("hooks")))).toBe("2 things need your attention");
  });

  it("says so when all is well", () => {
    expect(headline(report(ok("app")))).toBe("Dex is set up");
    expect(failures(report(ok("app")))).toEqual([]);
  });
});
