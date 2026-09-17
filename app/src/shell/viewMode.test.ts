import { describe, expect, it } from "vitest";
import { VIEW_MODES, chooseMode, modeOfAction, showsPanes, whenPanesHidden } from "./viewMode";

describe("chooseMode", () => {
  it("is the terminals until anyone says otherwise", () => {
    expect(chooseMode(null, undefined)).toBe("terminal");
  });

  it("opens as the owner's config says when they have never chosen", () => {
    expect(chooseMode(null, "office")).toBe("office");
    expect(chooseMode(null, "cards")).toBe("cards");
  });

  it("prefers what they last clicked over the config", () => {
    expect(chooseMode("terminal", "office")).toBe("terminal");
    expect(chooseMode("office", "cards")).toBe("office");
  });

  it("ignores a stored value, or a config from a newer Dex, that it does not understand", () => {
    expect(chooseMode("penthouse", "cards")).toBe("cards");
    expect(chooseMode(null, "hologram")).toBe("terminal");
  });
});

describe("the views", () => {
  it("are the three in the order they are offered", () => {
    expect(VIEW_MODES.map((mode) => mode.id)).toEqual(["terminal", "cards", "office"]);
    for (const mode of VIEW_MODES) expect(mode.label, mode.id).toBeTruthy();
  });

  it("show the panes in one of them only", () => {
    expect(showsPanes("terminal")).toBe(true);
    expect(showsPanes("cards")).toBe(false);
    expect(showsPanes("office")).toBe(false);
  });

  it("each have an action that selects them", () => {
    expect(modeOfAction("view-terminal")).toBe("terminal");
    expect(modeOfAction("view-cards")).toBe("cards");
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
    for (const kind of ["command-palette", "new-workspace", "toggle-sidebar", "next-workspace", "switch-workspace", "open-setup", "view-cards"]) {
      expect(whenPanesHidden(kind), kind).toBe("run");
    }
  });
});
