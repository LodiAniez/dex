import { describe, expect, it } from "vitest";
import { paneTitle, runsShell, shortPath } from "./paneKind";

const pane = (kind: string, cwd = "C:/Users/me/src/api") => ({ kind, cwd });

describe("paneTitle", () => {
  it("names the special panes by what they show", () => {
    expect(paneTitle(pane("activity"))).toBe("activity");
    expect(paneTitle(pane("office"))).toBe("office");
    expect(paneTitle(pane("diff"))).toBe("diff · …/src/api");
    expect(paneTitle(pane("markdown", "C:/notes/plan.md"))).toBe("plan.md");
  });

  it("names a terminal by where it is", () => {
    expect(paneTitle(pane("terminal"))).toBe("…/src/api");
  });
});

describe("shortPath", () => {
  it("keeps a short path whole", () => {
    expect(shortPath("C:/src")).toBe("C:/src");
  });
});

describe("runsShell", () => {
  it("is true for a terminal only", () => {
    expect(runsShell("terminal")).toBe(true);
    for (const kind of ["activity", "diff", "markdown", "office"]) {
      expect(runsShell(kind), kind).toBe(false);
    }
  });

  it("does not start a shell for a kind this build has never heard of", () => {
    // A newer daemon may know kinds this frontend does not; a shell nobody
    // asked for is the wrong way to find that out.
    expect(runsShell("hologram")).toBe(false);
  });
});
