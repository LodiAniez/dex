/**
 * The Activity box on the office floor, which the owner can minimize out of
 * the way. The choice is remembered by this window, like the view; what has
 * happened since it was minimized is counted on the button that reopens it.
 */

const STORAGE_KEY = "dex.office.activity";

export function isMinimized(stored: string | null): boolean {
  return stored === "minimized";
}

export function storedAs(minimized: boolean): string {
  return minimized ? "minimized" : "open";
}

/** The latest line's number, or 0 when nothing has been said. */
export function newest(lines: readonly { seq: number }[]): number {
  return lines.reduce((max, line) => Math.max(max, line.seq), 0);
}

/**
 * How many lines are newer than the last one seen. `seenUpTo` is null for a box
 * that was already minimized when the window opened: nothing is known to be new.
 */
export function unseen(lines: readonly { seq: number }[], seenUpTo: number | null): number {
  if (seenUpTo === null) return 0;
  return lines.filter((line) => line.seq > seenUpTo).length;
}

/** What the badge says. The box only holds so many lines, so a full box of new ones may be more. */
export function unseenLabel(count: number, capacity: number): string | null {
  if (count <= 0) return null;
  return count >= capacity ? `${count}+` : String(count);
}

/** What the owner last chose, if this window can remember anything. */
export function storedChoice(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

export function rememberChoice(minimized: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, storedAs(minimized));
  } catch {
    // Forgetting a preference is not worth telling anyone about.
  }
}
