import { describe, expect, it } from "vitest";
import { VIEW_MODES, chooseMode, modeOfAction, nextMode, showsPanes, whenPanesHidden } from "./viewMode";

describe("chooseMode", () => {
  it("is the terminals until anyone says otherwise", () => {
    expect(chooseMode(null, undefined)).toBe("terminal");
  });

  it("opens as the owner's config says when they have never chosen", () => {
    expect(chooseMode(null, "office")).toBe("office");
    expect(chooseMode(null, "terminal")).toBe("terminal");
  });

  it("prefers what they last clicked over the config", () => {
    expect(chooseMode("terminal", "office")).toBe("terminal");
    expect(chooseMode("office", "terminal")).toBe("office");
  });

  it("ignores a stored value, or a config from a newer Dex, that it does not understand", () => {
    expect(chooseMode("penthouse", "office")).toBe("office");
    expect(chooseMode(null, "hologram")).toBe("terminal");
  });

  it("forgets the cards view, which is gone: a window that remembered it opens as configured", () => {
    expect(chooseMode("cards", "office")).toBe("office");
    expect(chooseMode("cards", undefined)).toBe("terminal");
    expect(chooseMode(null, "cards")).toBe("terminal");
  });
});

describe("the views", () => {
  it("are the two in the order they are offered", () => {
    expect(VIEW_MODES.map((mode) => mode.id)).toEqual(["terminal", "office"]);
    for (const mode of VIEW_MODES) expect(mode.label, mode.id).toBeTruthy();
  });

  it("show the panes in one of them only", () => {
    expect(showsPanes("terminal")).toBe(true);
    expect(showsPanes("office")).toBe(false);
  });

  it("each have an action that selects them", () => {
    expect(modeOfAction("view-terminal")).toBe("terminal");
    expect(modeOfAction("view-cards")).toBeNull();
    expect(modeOfAction("view-office")).toBe("office");
    expect(modeOfAction("open-diff")).toBeNull();
  });
});

describe("whenPanesHidden", () => {
  it("never acts on a pane nobody can see, where acting could not be undone or seen", () => {
    // Closing a pane kills an agent's terminal. The first press only shows what
    // the second would do it to.
    for (const kind of ["close-pane", "move-pane", "focus-pane", "toggle-zoom"]) {
      expect(whenPanesHidden(kind), kind).toBe("reveal");
    }
  });

  it("shows the panes and then does what was asked, where that is harmless", () => {
    for (const kind of ["split-pane", "open-diff", "cycle-layout"]) {
      expect(whenPanesHidden(kind), kind).toBe("reveal-then-run");
    }
  });

  it("leaves alone everything that is not about a pane", () => {
    for (const kind of ["command-palette", "new-workspace", "toggle-sidebar", "next-workspace", "switch-workspace", "open-setup", "open-settings", "view-office"]) {
      expect(whenPanesHidden(kind), kind).toBe("run");
    }
  });
});

describe("nextMode", () => {
  it("goes back and forth between the two", () => {
    expect(nextMode("terminal")).toBe("office");
    expect(nextMode("office")).toBe("terminal");
  });
});

describe("the activity popup", () => {
  it("is not about a pane, so it opens over whatever view is up", () => {
    expect(whenPanesHidden("open-activity")).toBe("run");
  });
});
