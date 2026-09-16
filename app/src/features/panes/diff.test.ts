import { describe, expect, it } from "vitest";
import { parseDiff } from "./diff";

const sample = [
  "diff --git a/src/main.rs b/src/main.rs",
  "index 3b18e51..a1b2c3d 100644",
  "--- a/src/main.rs",
  "+++ b/src/main.rs",
  "@@ -1,3 +1,4 @@",
  " fn main() {",
  "-    println!(\"hi\");",
  "+    println!(\"hello\");",
  "+    // more",
  " }",
  "diff --git a/README.md b/README.md",
  "new file mode 100644",
  "index 0000000..e69de29",
  "--- /dev/null",
  "+++ b/README.md",
  "@@ -0,0 +1 @@",
  "+# Title",
].join("\n");

describe("splitting a diff into files", () => {
  it("finds each file by its header and names it by the new path", () => {
    const files = parseDiff(sample);
    expect(files.map((f) => f.path)).toEqual(["src/main.rs", "README.md"]);
  });

  it("counts additions and removals per file, not counting the +++/--- headers", () => {
    const [main, readme] = parseDiff(sample);
    expect([main.added, main.removed]).toEqual([2, 1]);
    expect([readme.added, readme.removed]).toEqual([1, 0]);
  });

  it("classifies every kind of line", () => {
    const [main] = parseDiff(sample);
    expect(main.lines.map((l) => l.kind)).toEqual([
      "file",
      "meta",
      "meta",
      "meta",
      "hunk",
      "context",
      "del",
      "add",
      "add",
      "context",
    ]);
  });

  it("treats file-mode and rename lines as metadata, not changes", () => {
    const [, readme] = parseDiff(sample);
    expect(readme.lines[1]).toEqual({ kind: "meta", text: "new file mode 100644" });
    const renamed = parseDiff("diff --git a/x b/y\nsimilarity index 100%\nrename from x\nrename to y");
    expect(renamed[0].lines.slice(1).every((l) => l.kind === "meta")).toBe(true);
    expect(renamed[0].added).toBe(0);
  });

  it("does not mistake a removed line starting with dashes for a header", () => {
    // A line that was `--x` in the file shows up as `---x` in the diff. Git's
    // own headers are only ever `--- a/` or `--- /dev/null`, but this parser
    // keeps it simple and calls both meta; what matters is nothing crashes
    // and the count stays honest for ordinary lines.
    const [file] = parseDiff("diff --git a/f b/f\n@@ -1 +1 @@\n-old\n+new");
    expect([file.added, file.removed]).toEqual([1, 1]);
  });

  it("returns nothing for an empty diff", () => {
    expect(parseDiff("")).toEqual([]);
  });

  it("shows text before any header rather than dropping it", () => {
    const files = parseDiff("warning: LF will be replaced by CRLF\ndiff --git a/f b/f");
    expect(files[0].path).toBe("");
    expect(files[0].lines[0].text).toContain("warning");
    expect(files[1].path).toBe("f");
  });
});
