import { describe, expect, it } from "vitest";
import {
  type Check,
  type DoctorReport,
  distroToSetUp,
  failures,
  fingerprint,
  headline,
  shouldOffer,
  stepFor,
  stepLabel,
} from "./setup";

function report(...checks: Check[]): DoctorReport {
  return { ok: checks.every((c) => c.status !== "fail"), checks };
}

const ok = (name: string): Check => ({ name, status: "ok", detail: "" });
const fail = (name: string, detail = ""): Check => ({ name, status: "fail", detail });
const skip = (name: string): Check => ({ name, status: "skip", detail: "" });

describe("which failures have a button", () => {
  it("offers to install hooks, register the MCP server, and install the skill", () => {
    expect(stepFor(fail("hooks"))).toBe("hooks");
    expect(stepFor(fail("mcp"))).toBe("mcp");
    expect(stepFor(fail("skill"))).toBe("skill");
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

describe("a WSL distro in the setup panel", () => {
  const distro = (status: "ok" | "fail" | "skip", fix?: string) => ({ name: "wsl:Ubuntu", status, detail: "", fix });

  it("gets a button that sets it up, whether or not Dex uses it yet", () => {
    expect(stepFor(distro("fail", "wsl setup Ubuntu"))).toBe("wsl:Ubuntu");
    expect(stepFor(distro("skip", "wsl setup Ubuntu"))).toBe("wsl:Ubuntu");
    expect(stepFor(distro("ok"))).toBeNull();
  });

  it("is labelled with the distro's name", () => {
    expect(stepLabel("wsl:Ubuntu")).toBe("Set up Ubuntu");
    expect(stepLabel("hooks")).toBe("Install hooks");
  });

  it("does not open the panel by itself until Dex runs something there", () => {
    const report = { ok: true, checks: [{ name: "hooks", status: "ok" as const, detail: "" }, distro("skip", "wsl setup Ubuntu")] };
    expect(shouldOffer(report, null)).toBe(false);
  });
});

describe("the chosen terminal's distro, in settings", () => {
  const distro = (name: string, status: "ok" | "fail" | "skip"): Check => ({
    name: `wsl:${name}`,
    status,
    detail: "",
    fix: status === "ok" ? undefined : `wsl setup ${name}`,
  });

  it("names a chosen distro that is not set up for agents yet", () => {
    expect(distroToSetUp(report(ok("hooks"), distro("Ubuntu", "fail")), "wsl:Ubuntu")).toBe("Ubuntu");
    // Just chosen: doctor has not counted it as in use yet.
    expect(distroToSetUp(report(distro("Ubuntu", "skip")), "wsl:Ubuntu")).toBe("Ubuntu");
  });

  it("names nothing once it is set up", () => {
    expect(distroToSetUp(report(distro("Ubuntu", "ok")), "wsl:Ubuntu")).toBeNull();
  });

  it("names nothing for PowerShell, or for another distro that is not set up", () => {
    const checks = report(distro("Ubuntu", "fail"), distro("Debian", "ok"));
    expect(distroToSetUp(checks, "windows")).toBeNull();
    expect(distroToSetUp(checks, "wsl:Debian")).toBeNull();
  });

  it("names nothing doctor has not checked", () => {
    expect(distroToSetUp(report(ok("hooks")), "wsl:Ubuntu")).toBeNull();
    expect(distroToSetUp(null, "wsl:Ubuntu")).toBeNull();
  });
});
