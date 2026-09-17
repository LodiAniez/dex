import { describe, expect, it } from "vitest";
import { VIEW_MODES, chooseMode, modeOfAction, showsPanes } from "./viewMode";

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
