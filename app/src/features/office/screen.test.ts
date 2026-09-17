import { describe, expect, it } from "vitest";
import { followsBottom, lastLines, screenText } from "./screen";

const ESC = String.fromCharCode(27);
const BEL = String.fromCharCode(7);

describe("lastLines", () => {
  it("returns the last lines with something on them", () => {
    expect(lastLines("one\r\ntwo\r\n\r\nthree\r\n\r\n", 2)).toEqual(["two", "three"]);
  });

  it("drops colour and cursor escapes", () => {
    const text = `${ESC}[32mtest ok${ESC}[0m\r\n${ESC}[2K${ESC}[1Gdone`;
    expect(lastLines(text, 2)).toEqual(["test ok", "done"]);
  });

  it("drops a window-title sequence whole", () => {
    expect(lastLines(`${ESC}]0;claude${BEL}hello`, 1)).toEqual(["hello"]);
  });

  it("keeps indentation but not trailing space", () => {
    expect(lastLines("  [y] yes  [n] no   ", 1)).toEqual(["  [y] yes  [n] no"]);
  });

  it("is empty for a pane with no terminal, or nothing on it", () => {
    expect(lastLines(undefined, 3)).toEqual([]);
    expect(lastLines(" \r\n ", 3)).toEqual([]);
  });

  it("does not pad when there is less than was asked for", () => {
    expect(lastLines("only", 5)).toEqual(["only"]);
  });
});

describe("screenText", () => {
  it("keeps the blank lines inside the output, which are part of how it reads", () => {
    expect(screenText("one\n\ntwo\n", 10)).toEqual(["one", "", "two"]);
  });

  it("drops the empty rows below the last thing printed, and above the first", () => {
    expect(screenText("\n\nfirst\nsecond\n\n\n\n", 10)).toEqual(["first", "second"]);
  });

  it("keeps the newest lines when there are more than were asked for", () => {
    expect(screenText("a\nb\nc\nd", 2)).toEqual(["c", "d"]);
  });

  it("is empty for a pane with no terminal, or nothing on it", () => {
    expect(screenText(undefined, 5)).toEqual([]);
    expect(screenText("  \n \n", 5)).toEqual([]);
  });
});

describe("followsBottom", () => {
  it("follows new output while the reader is at the bottom", () => {
    expect(followsBottom({ scrollTop: 600, clientHeight: 400, scrollHeight: 1000 })).toBe(true);
  });

  it("allows for a few pixels of rounding", () => {
    expect(followsBottom({ scrollTop: 590, clientHeight: 400, scrollHeight: 1000 })).toBe(true);
  });

  it("leaves the reader where they are once they have scrolled up to read", () => {
    expect(followsBottom({ scrollTop: 300, clientHeight: 400, scrollHeight: 1000 })).toBe(false);
  });

  it("follows when everything fits and there is nothing to scroll", () => {
    expect(followsBottom({ scrollTop: 0, clientHeight: 400, scrollHeight: 300 })).toBe(true);
  });
});
