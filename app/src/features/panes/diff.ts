/**
 * Classifying a unified diff for display: which lines are file headers, hunk
 * headers, additions, removals, and context. Pure; the pane colours by `kind`.
 */

export type DiffLineKind = "file" | "meta" | "hunk" | "add" | "del" | "context";

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
}

export interface DiffFile {
  /** The path after the change, or before it for a deletion. */
  path: string;
  lines: DiffLine[];
  added: number;
  removed: number;
}

/** `diff --git a/x b/x` → `x`; falls back to whatever follows the `b/`. */
function pathOf(header: string): string {
  const match = /^diff --git a\/(.*?) b\/(.*)$/.exec(header);
  return match ? match[2] : header.replace(/^diff --git /, "");
}

/** Splits a unified diff into files, each with its classified lines. */
export function parseDiff(text: string): DiffFile[] {
  const files: DiffFile[] = [];
  let current: DiffFile | null = null;
  for (const raw of text.split("\n")) {
    if (raw.startsWith("diff --git ")) {
      current = { path: pathOf(raw), lines: [{ kind: "file", text: raw }], added: 0, removed: 0 };
      files.push(current);
      continue;
    }
    if (!current) {
      // Output before the first file header — a warning, say. Show it plainly.
      if (raw.trim() === "") continue;
      current = { path: "", lines: [], added: 0, removed: 0 };
      files.push(current);
    }
    current.lines.push(classify(raw, current));
  }
  return files;
}

function classify(line: string, file: DiffFile): DiffLine {
  if (line.startsWith("@@")) return { kind: "hunk", text: line };
  if (line.startsWith("+++") || line.startsWith("---")) return { kind: "meta", text: line };
  if (
    line.startsWith("index ") ||
    line.startsWith("new file mode") ||
    line.startsWith("deleted file mode") ||
    line.startsWith("similarity index") ||
    line.startsWith("rename ") ||
    line.startsWith("old mode") ||
    line.startsWith("new mode") ||
    line.startsWith("Binary files")
  ) {
    return { kind: "meta", text: line };
  }
  if (line.startsWith("+")) {
    file.added += 1;
    return { kind: "add", text: line };
  }
  if (line.startsWith("-")) {
    file.removed += 1;
    return { kind: "del", text: line };
  }
  return { kind: "context", text: line };
}
