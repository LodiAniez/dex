import { describe, expect, it } from "vitest";
import { parseInline, parseMarkdown } from "./markdown";

const text = (s: string) => ({ kind: "text", text: s });

describe("blocks", () => {
  it("reads headings at every level and strips closing hashes", () => {
    expect(parseMarkdown("# One\n### Three ###")).toEqual([
      { kind: "heading", level: 1, children: [text("One")] },
      { kind: "heading", level: 3, children: [text("Three")] },
    ]);
  });

  it("joins a paragraph's lines and splits paragraphs on blank lines", () => {
    expect(parseMarkdown("first line\nsecond line\n\nnext")).toEqual([
      { kind: "paragraph", children: [text("first line second line")] },
      { kind: "paragraph", children: [text("next")] },
    ]);
  });

  it("keeps fenced code exactly, with its language", () => {
    const blocks = parseMarkdown("```rust\nfn main() {}\n\n  indented\n```\nafter");
    expect(blocks[0]).toEqual({ kind: "code", lang: "rust", text: "fn main() {}\n\n  indented" });
    expect(blocks[1]).toEqual({ kind: "paragraph", children: [text("after")] });
  });

  it("does not parse markdown inside a code fence", () => {
    const [code] = parseMarkdown("```\n# not a heading\n- not a list\n```");
    expect(code).toEqual({ kind: "code", lang: null, text: "# not a heading\n- not a list" });
  });

  it("closes an unterminated fence at the end of the text", () => {
    // A note being written has an open fence half the time; the words still show.
    expect(parseMarkdown("```\nstill typing")).toEqual([{ kind: "code", lang: null, text: "still typing" }]);
  });

  it("reads lists with nesting, numbering, and task boxes", () => {
    const [list] = parseMarkdown("- a\n  - b\n- [x] done\n- [ ] todo");
    expect(list).toEqual({
      kind: "list",
      ordered: false,
      items: [
        { depth: 0, checked: null, children: [text("a")] },
        { depth: 1, checked: null, children: [text("b")] },
        { depth: 0, checked: true, children: [text("done")] },
        { depth: 0, checked: false, children: [text("todo")] },
      ],
    });
    const [ordered] = parseMarkdown("1. one\n2) two");
    expect(ordered).toMatchObject({ kind: "list", ordered: true });
  });

  it("tells a rule from a list", () => {
    expect(parseMarkdown("---")).toEqual([{ kind: "rule" }]);
    expect(parseMarkdown("- item")[0].kind).toBe("list");
  });

  it("joins consecutive quote lines", () => {
    expect(parseMarkdown("> one\n> two")).toEqual([{ kind: "quote", children: [text("one two")] }]);
  });

  it("reads a pipe table with a header", () => {
    const [table] = parseMarkdown("| a | b |\n|---|:--:|\n| 1 | 2 |\n| 3 | 4 |");
    expect(table).toEqual({
      kind: "table",
      header: [[text("a")], [text("b")]],
      rows: [
        [[text("1")], [text("2")]],
        [[text("3")], [text("4")]],
      ],
    });
  });

  it("treats a lone pipe line without a rule as a paragraph", () => {
    expect(parseMarkdown("| not | a table |")[0].kind).toBe("paragraph");
  });

  it("ends a paragraph where a heading or list starts", () => {
    const blocks = parseMarkdown("text\n# head\n- item");
    expect(blocks.map((b) => b.kind)).toEqual(["paragraph", "heading", "list"]);
  });

  it("accepts Windows line endings", () => {
    expect(parseMarkdown("# a\r\n\r\nb")).toEqual([
      { kind: "heading", level: 1, children: [text("a")] },
      { kind: "paragraph", children: [text("b")] },
    ]);
  });

  it("produces nothing for empty text", () => {
    expect(parseMarkdown("")).toEqual([]);
    expect(parseMarkdown("\n\n  \n")).toEqual([]);
  });
});

describe("inline", () => {
  it("reads code spans first, so their contents are never emphasis", () => {
    expect(parseInline("use `a*b*c` here")).toEqual([
      text("use "),
      { kind: "code", text: "a*b*c" },
      text(" here"),
    ]);
  });

  it("reads strong and emphasis, nested", () => {
    expect(parseInline("**bold** and *it* and __b__ and _i_")).toEqual([
      { kind: "strong", children: [text("bold")] },
      text(" and "),
      { kind: "em", children: [text("it")] },
      text(" and "),
      { kind: "strong", children: [text("b")] },
      text(" and "),
      { kind: "em", children: [text("i")] },
    ]);
    expect(parseInline("**bold *and* it**")).toEqual([
      { kind: "strong", children: [text("bold "), { kind: "em", children: [text("and")] }, text(" it")] },
    ]);
  });

  it("does not treat an underscore inside a word as emphasis", () => {
    expect(parseInline("snake_case_name")).toEqual([text("snake_case_name")]);
    expect(parseInline("a * b * c")).toEqual([text("a * b * c")]);
  });

  it("reads links with safe schemes and flattens the rest to text", () => {
    expect(parseInline("[docs](https://example.com/x)")).toEqual([
      { kind: "link", children: [text("docs")], href: "https://example.com/x" },
    ]);
    expect(parseInline("[rel](./notes.md)")).toEqual([{ kind: "link", children: [text("rel")], href: "./notes.md" }]);
    // No scheme Dex will not follow survives as a link.
    expect(parseInline("[x](javascript:void)")).toEqual([text("x")]);
    expect(parseInline("[x](file:///C:/secret)")).toEqual([text("x")]);
  });

  it("leaves an unclosed marker as text", () => {
    expect(parseInline("a **b")).toEqual([text("a **b")]);
    expect(parseInline("`open")).toEqual([text("`open")]);
    expect(parseInline("[no](close")).toEqual([text("[no](close")]);
  });
});
