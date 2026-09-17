import { describe, expect, it } from "vitest";
import { lastLines } from "./screen";

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
