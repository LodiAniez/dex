import { describe, expect, it } from "vitest";
import { joinWrapped } from "./bufferText";

const row = (text: string, wrapped = false) => ({ text, wrapped });

describe("joinWrapped", () => {
  it("puts back together a line the terminal wrapped at its width", () => {
    // 10 columns wide: one logical line, three rows.
    const rows = [row("0123456789"), row("abcdefghij", true), row("XYZ       ", true)];
    expect(joinWrapped(rows)).toEqual(["0123456789abcdefghijXYZ"]);
  });

  it("keeps the spaces a wrap fell on, which are part of the line", () => {
    expect(joinWrapped([row("hello     "), row("world", true)])).toEqual(["hello     world"]);
  });

  it("leaves lines that were never wrapped alone, trailing space dropped", () => {
    expect(joinWrapped([row("one   "), row(""), row("two")])).toEqual(["one", "", "two"]);
  });

  it("copes with a first row that claims to be a continuation", () => {
    // The start of the line scrolled out of the buffer.
    expect(joinWrapped([row("tail", true), row("next")])).toEqual(["tail", "next"]);
  });

  it("is empty for an empty buffer", () => {
    expect(joinWrapped([])).toEqual([]);
  });
});
