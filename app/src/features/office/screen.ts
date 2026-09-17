/**
 * What is on an agent's screen, as plain lines: the terminal's serialized
 * buffer with its escapes removed. Shown on a card and in the work panel, so
 * "their work" is what the agent is really printing, not a description of it.
 */

const ESC = String.fromCharCode(27);
const BEL = String.fromCharCode(7);

/** OSC (title and the like, ended by BEL or ESC \), then CSI, then two-byte escapes. */
const ESCAPES = new RegExp(
  `${ESC}\\][^${BEL}${ESC}]*(?:${BEL}|${ESC}\\\\)|${ESC}\\[[0-9;?]*[ -/]*[@-~]|${ESC}[@-Z\\\\-_]`,
  "g",
);

/** The last `count` lines that have something on them, oldest first. */
export function lastLines(captured: string | undefined, count: number): string[] {
  if (!captured) return [];
  return captured
    .replace(ESCAPES, "")
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.trim() !== "")
    .slice(-count);
}

/** Whether two readings of a screen say the same thing. */
export function sameLines(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((line, i) => line === b[i]);
}
