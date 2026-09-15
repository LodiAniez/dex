export type Direction = "left" | "right" | "up" | "down";

/** App-level actions bound to keys (PRD §13). Everything else goes to the terminal. */
export type AppAction =
  | { kind: "new-workspace" }
  | { kind: "toggle-sidebar" }
  | { kind: "switch-workspace"; index: number }
  | { kind: "next-workspace" }
  | { kind: "previous-workspace" }
  | { kind: "split-pane"; direction: "right" | "down" }
  | { kind: "close-pane" }
  | { kind: "focus-pane"; direction: Direction }
  | { kind: "move-pane"; direction: Direction }
  | { kind: "toggle-zoom" }
  | { kind: "cycle-layout" };

const ARROWS: Partial<Record<string, Direction>> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
};

/**
 * The physical key, layout-independent: "N", "1", "PageDown". Taken from
 * `code` when present; synthetic input (SendKeys, remote desktop, remappers)
 * often has no scan code and so an empty `code`, and then `key` is used.
 */
function keyName(event: KeyboardEvent): string {
  if (event.code.startsWith("Key")) return event.code.slice(3);
  if (event.code.startsWith("Digit")) return event.code.slice(5);
  if (event.code === "NumpadEnter") return "Enter";
  if (event.code) return event.code;
  if (event.key === " ") return "Space";
  return event.key.length === 1 ? event.key.toUpperCase() : event.key;
}

/**
 * The app action for a key event, or null if the terminal should get the key.
 * Plain Ctrl+letter always belongs to the terminal (Ctrl+D is EOF, Ctrl+W
 * deletes a word, Ctrl+[ is Escape), so app shortcuts use Ctrl+Shift, Alt+Arrow,
 * and Ctrl+digit, following Windows Terminal.
 */
export function appActionFor(event: KeyboardEvent): AppAction | null {
  if (event.metaKey) return null;
  const key = keyName(event);

  if (event.altKey && !event.ctrlKey) {
    const direction = ARROWS[key];
    if (!direction) return null;
    return event.shiftKey ? { kind: "move-pane", direction } : { kind: "focus-pane", direction };
  }
  if (event.altKey || !event.ctrlKey) return null;

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
    case "D":
      return { kind: "split-pane", direction: "right" };
    case "E":
      return { kind: "split-pane", direction: "down" };
    case "W":
      return { kind: "close-pane" };
    case "Enter":
      return { kind: "toggle-zoom" };
    case "Space":
      return { kind: "cycle-layout" };
    default:
      return null;
  }
}
