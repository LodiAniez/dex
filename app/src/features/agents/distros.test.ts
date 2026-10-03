import { describe, expect, it } from "vitest";
import { newlyQuiet } from "./distros";

describe("newlyQuiet", () => {
  it("says which distro stopped answering", () => {
    const said = newlyQuiet([], ["Ubuntu"]);
    expect(said).toContain("Ubuntu");
    expect(said).toContain("not answering");
  });

  it("says it once, not every sweep", () => {
    // The daemon reports the same list for as long as the silence lasts.
    expect(newlyQuiet(["Ubuntu"], ["Ubuntu"])).toBeNull();
  });

  it("says nothing while every distro answers", () => {
    expect(newlyQuiet([], [])).toBeNull();
    expect(newlyQuiet(undefined, undefined)).toBeNull();
    expect(newlyQuiet(["Ubuntu"], [])).toBeNull();
  });

  it("speaks up again when a second distro goes quiet", () => {
    const said = newlyQuiet(["Ubuntu"], ["Ubuntu", "Debian"]);
    expect(said).toContain("Debian");
    expect(said).not.toContain("Ubuntu");
  });

  it("names several at once in one line", () => {
    const said = newlyQuiet(undefined, ["Debian", "Ubuntu"]);
    expect(said).toContain("Debian, Ubuntu");
    expect(said).toContain("are not answering");
  });

  it("tells the owner their agents may be fine", () => {
    // The point of the line: a still office is not the same as a dead agent.
    expect(newlyQuiet([], ["Ubuntu"])).toContain("may still be working");
  });
});
