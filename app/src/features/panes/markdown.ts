/**
 * A small markdown parser for the markdown pane: text in, a tree of blocks
 * out, nothing else. The renderer turns the tree into React elements, so no
 * HTML is ever produced from the file — an agent-written note cannot inject
 * markup, because there is no markup to inject into.
 *
 * Covers what notes and the context mirror use: headings, paragraphs, fenced
 * code, lists (nested, ordered, task boxes), quotes, rules, pipe tables, and
 * inline code, emphasis and links. Anything it does not understand is a
 * paragraph, which is the right failure: the words still show.
 */

export type Inline =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strong"; children: Inline[] }
  | { kind: "em"; children: Inline[] }
  | { kind: "link"; children: Inline[]; href: string };

export interface ListItem {
  depth: number;
  /** Set for `- [ ]` and `- [x]` items. */
  checked: boolean | null;
  children: Inline[];
}

export type Block =
  | { kind: "heading"; level: number; children: Inline[] }
  | { kind: "paragraph"; children: Inline[] }
  | { kind: "code"; lang: string | null; text: string }
  | { kind: "list"; ordered: boolean; items: ListItem[] }
  | { kind: "quote"; children: Inline[] }
  | { kind: "rule" }
  | { kind: "table"; header: Inline[][]; rows: Inline[][][] };

const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const FENCE = /^(`{3,}|~{3,})\s*([\w+-]*)\s*$/;
const RULE = /^(?:-{3,}|\*{3,}|_{3,})\s*$/;
const BULLET = /^(\s*)([-*+]|\d+[.)])\s+(?:\[([ xX])\]\s+)?(.*)$/;
const QUOTE = /^>\s?(.*)$/;
const TABLE_ROW = /^\s*\|.*\|\s*$/;
const TABLE_RULE = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

/** Parses a whole document. */
export function parseMarkdown(text: string): Block[] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i += 1;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const close = fence[1];
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].startsWith(close)) body.push(lines[i++]);
      i += 1; // the closing fence, or the end of the text
      blocks.push({ kind: "code", lang: fence[2] || null, text: body.join("\n") });
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({ kind: "heading", level: heading[1].length, children: parseInline(heading[2]) });
      i += 1;
      continue;
    }

    if (RULE.test(line)) {
      blocks.push({ kind: "rule" });
      i += 1;
      continue;
    }

    if (QUOTE.test(line)) {
      const parts: string[] = [];
      while (i < lines.length && QUOTE.test(lines[i])) parts.push(QUOTE.exec(lines[i++])![1]);
      blocks.push({ kind: "quote", children: parseInline(parts.join(" ")) });
      continue;
    }

    if (BULLET.test(line)) {
      const items: ListItem[] = [];
      const ordered = /\d/.test(BULLET.exec(line)![2]);
      while (i < lines.length && BULLET.test(lines[i])) {
        const [, indent, , box, rest] = BULLET.exec(lines[i])!;
        items.push({
          depth: Math.floor(indent.replace(/\t/g, "  ").length / 2),
          checked: box === undefined ? null : box !== " ",
          children: parseInline(rest),
        });
        i += 1;
      }
      blocks.push({ kind: "list", ordered, items });
      continue;
    }

    if (TABLE_ROW.test(line) && i + 1 < lines.length && TABLE_RULE.test(lines[i + 1])) {
      const header = cells(line);
      i += 2;
      const rows: Inline[][][] = [];
      while (i < lines.length && TABLE_ROW.test(lines[i])) rows.push(cells(lines[i++]));
      blocks.push({ kind: "table", header, rows });
      continue;
    }

    // A paragraph runs until a blank line or something that is clearly not
    // paragraph text. Lines are joined with spaces, as markdown does.
    const parts: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !startsBlock(lines[i])) parts.push(lines[i++].trim());
    if (parts.length === 0) parts.push(lines[i++].trim()); // never loop forever on a line nothing claims
    blocks.push({ kind: "paragraph", children: parseInline(parts.join(" ")) });
  }
  return blocks;
}

function startsBlock(line: string): boolean {
  return FENCE.test(line) || HEADING.test(line) || RULE.test(line) || QUOTE.test(line) || BULLET.test(line);
}

function cells(row: string): Inline[][] {
  return row
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => parseInline(cell.trim()));
}

/** Only these link schemes are kept; anything else becomes plain text. */
const SAFE_HREF = /^(https?:\/\/|mailto:|#|\.{0,2}\/|[\w.-]+(\/|$))/i;

/** Parses one line's worth of inline markdown. */
export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  let plain = "";
  const flush = () => {
    if (plain) out.push({ kind: "text", text: plain });
    plain = "";
  };
  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);

    if (rest.startsWith("`")) {
      const ticks = /^`+/.exec(rest)![0];
      const end = rest.indexOf(ticks, ticks.length);
      if (end > 0) {
        flush();
        out.push({ kind: "code", text: rest.slice(ticks.length, end).trim() });
        i += end + ticks.length;
        continue;
      }
    }

    // `*?` then `\S`: the content may be a single character, as in `**b**`.
    const strong = /^(\*\*|__)(?=\S)([\s\S]*?\S)\1/.exec(rest);
    if (strong) {
      flush();
      out.push({ kind: "strong", children: parseInline(strong[2]) });
      i += strong[0].length;
      continue;
    }

    const em = /^([*_])(?=\S)([^*_]*?\S)\1(?![*_\w])/.exec(rest);
    if (em) {
      flush();
      out.push({ kind: "em", children: parseInline(em[2]) });
      i += em[0].length;
      continue;
    }

    const link = /^\[([^\]]+)\]\(([^)\s]+)\)/.exec(rest);
    if (link) {
      flush();
      if (SAFE_HREF.test(link[2])) out.push({ kind: "link", children: parseInline(link[1]), href: link[2] });
      else out.push({ kind: "text", text: link[1] });
      i += link[0].length;
      continue;
    }

    plain += text[i];
    i += 1;
  }
  flush();
  return out;
}
