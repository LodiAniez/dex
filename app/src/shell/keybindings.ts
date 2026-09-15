/** App-level actions bound to keys (PRD §13). Everything else goes to the terminal. */
export type AppAction =
  | { kind: "new-workspace" }
  | { kind: "toggle-sidebar" }
  | { kind: "switch-workspace"; index: number }
  | { kind: "next-workspace" }
  | { kind: "previous-workspace" };

/**
 * The physical key, layout-independent: "N", "1", "PageDown". Taken from
 * `code` when present; synthetic input (SendKeys, remote desktop, remappers)
 * often has no scan code and so an empty `code`, and then `key` is used.
 */
function keyName(event: KeyboardEvent): string {
  if (event.code.startsWith("Key")) return event.code.slice(3);
  if (event.code.startsWith("Digit")) return event.code.slice(5);
  if (event.code) return event.code;
  return event.key.length === 1 ? event.key.toUpperCase() : event.key;
}

/**
 * The app action for a key event, or null if the terminal should get the key.
 * Plain Ctrl+letter always belongs to the terminal (Ctrl+D is EOF, Ctrl+W
 * deletes a word, Ctrl+[ is Escape), so app shortcuts use Ctrl+Shift, and
 * Ctrl+digit, which terminals do not use.
 */
export function appActionFor(event: KeyboardEvent): AppAction | null {
  if (event.altKey || event.metaKey || !event.ctrlKey) return null;
  const key = keyName(event);
  if (!event.shiftKey) {
    return /^[1-9]$/.test(key) ? { kind: "switch-workspace", index: Number(key) - 1 } : null;
  }
  switch (key) {
    case "N":
      return { kind: "new-workspace" };
    case "B":
      return { kind: "toggle-sidebar" };
    case "PageDown":
      return { kind: "next-workspace" };
    case "PageUp":
      return { kind: "previous-workspace" };
    default:
      return null;
  }
}
