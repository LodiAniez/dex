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

/**
 * An agent's output as it reads on their terminal: up to `count` of the newest
 * lines, blank lines inside it kept - they are part of how output reads - and
 * the empty rows above and below it dropped. For the expanded view; a card has
 * no room for blank lines and uses `lastLines`.
 */
export function screenText(captured: string | undefined, count: number): string[] {
  if (!captured) return [];
  const lines = captured
    .replace(ESCAPES, "")
    .split(/\r?\n/)
    .map((line) => line.trimEnd());
  const first = lines.findIndex((line) => line !== "");
  if (first < 0) return [];
  let last = lines.length - 1;
  while (lines[last] === "") last -= 1;
  return lines.slice(first, last + 1).slice(-count);
}

/** A few pixels of slack: scroll positions are fractional, and "at the bottom" should not need to be exact. */
const BOTTOM_SLACK = 24;

/**
 * Whether a scrolling view of live output should keep following the newest
 * line. It does while the reader is at the bottom; once they scroll up to read
 * something, it leaves them there until they come back down.
 */
export function followsBottom(view: { scrollTop: number; clientHeight: number; scrollHeight: number }): boolean {
  return view.scrollHeight - view.scrollTop - view.clientHeight <= BOTTOM_SLACK;
}
