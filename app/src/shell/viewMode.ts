/**
 * The three ways of looking at a workspace. `terminal` is the panes, as Dex
 * has always shown them; `cards` and `office` show the same workspace's agents
 * instead, over the whole workspace area. The panes keep running underneath:
 * a view is a way of looking, not a place agents live.
 */
export type ViewMode = "terminal" | "cards" | "office";

/** In the order the title bar offers them. */
export const VIEW_MODES: readonly { id: ViewMode; label: string; hint: string }[] = [
  { id: "terminal", label: "Terminal", hint: "The panes" },
  { id: "cards", label: "Cards", hint: "Every agent as a card" },
  { id: "office", label: "Office", hint: "Every agent at a desk" },
];

const STORAGE_KEY = "dex.view";

function isMode(value: string | null | undefined): value is ViewMode {
  return VIEW_MODES.some((mode) => mode.id === value);
}

/**
 * Which view to show: what the owner last chose, else `[ui] view` from their
 * config, else the terminals. The choice is remembered here in the window and
 * never written to their config file, which stays theirs.
 */
export function chooseMode(stored: string | null, configured: string | undefined): ViewMode {
  if (isMode(stored)) return stored;
  return isMode(configured) ? configured : "terminal";
}

/** Whether the panes themselves are what is on screen. */
export function showsPanes(mode: ViewMode): boolean {
  return mode === "terminal";
}

/** The view an app action selects, or null for any other action. */
export function modeOfAction(action: string): ViewMode | null {
  const id = action.startsWith("view-") ? action.slice("view-".length) : null;
  return isMode(id) ? id : null;
}

/** What the owner last chose, if this window can remember anything. */
export function storedMode(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

export function rememberMode(mode: ViewMode): void {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // Forgetting a preference is not worth telling anyone about.
  }
}
