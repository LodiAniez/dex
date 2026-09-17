import { describe, expect, it } from "vitest";
import { chooseView } from "./officeView";

describe("chooseView", () => {
  it("opens as the owner's config says when they have never chosen", () => {
    expect(chooseView(null, "office")).toBe("office");
    expect(chooseView(null, "cards")).toBe("cards");
  });

  it("prefers what they last clicked over the config", () => {
    expect(chooseView("office", "cards")).toBe("office");
    expect(chooseView("cards", "office")).toBe("cards");
  });

  it("ignores a stored value it does not understand", () => {
    expect(chooseView("penthouse", "office")).toBe("office");
  });

  it("falls back to cards when the config has not arrived, or is from a newer Dex", () => {
    expect(chooseView(null, undefined)).toBe("cards");
    expect(chooseView(null, "hologram")).toBe("cards");
  });
});
