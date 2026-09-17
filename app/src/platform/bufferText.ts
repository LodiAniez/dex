/**
 * A terminal's rows as the lines that were printed. A terminal wraps a long
 * line at its width and marks each continuation row; read row by row, one line
 * of output comes back as several, broken wherever the window happened to end.
 * This puts them back together.
 */

export interface BufferRow {
  /** The row's text, untrimmed: a wrap can fall on a space, which is part of the line. */
  text: string;
  /** Whether this row continues the one above it. */
  wrapped: boolean;
}

export function joinWrapped(rows: readonly BufferRow[]): string[] {
  const lines: string[] = [];
  for (const row of rows) {
    // A first row that claims to continue something has lost its beginning to
    // scrollback; it starts a line of its own.
    if (row.wrapped && lines.length > 0) lines[lines.length - 1] += row.text;
    else lines.push(row.text);
  }
  return lines.map((line) => line.trimEnd());
}
