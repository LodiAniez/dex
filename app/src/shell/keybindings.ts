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
  | { kind: "cycle-layout" }
  | { kind: "command-palette" }
  | { kind: "open-diff" }
  | { kind: "open-office" }
  | { kind: "open-setup" };

const DIRECTIONS: Direction[] = ["left", "right", "up", "down"];

/**
 * Every action an owner may rebind, by the name they write in `[keys]`.
 *
 * The defaults live here rather than in the daemon's config so the table has
 * one owner: the config file carries overrides only, and an action nobody
 * overrode keeps whatever this ships with.
 */
export const ACTIONS: Record<string, AppAction> = {
  "command-palette": { kind: "command-palette" },
  "new-workspace": { kind: "new-workspace" },
  "toggle-sidebar": { kind: "toggle-sidebar" },
  "next-workspace": { kind: "next-workspace" },
  "previous-workspace": { kind: "previous-workspace" },
  "split-right": { kind: "split-pane", direction: "right" },
  "split-down": { kind: "split-pane", direction: "down" },
  "close-pane": { kind: "close-pane" },
  "toggle-zoom": { kind: "toggle-zoom" },
  "cycle-layout": { kind: "cycle-layout" },
  // Palette-only unless the owner binds them: PRD §13 gives them no key.
  "open-diff": { kind: "open-diff" },
  "open-office": { kind: "open-office" },
  "open-setup": { kind: "open-setup" },
  ...Object.fromEntries(
    DIRECTIONS.flatMap((direction): [string, AppAction][] => [
      [`focus-${direction}`, { kind: "focus-pane", direction }],
      [`move-${direction}`, { kind: "move-pane", direction }],
    ]),
  ),
  ...Object.fromEntries(
    [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n): [string, AppAction] => [
      `switch-workspace-${n}`,
      { kind: "switch-workspace", index: n - 1 },
    ]),
  ),
};

/**
 * The shipped bindings (PRD §13). App commands avoid plain `Ctrl+<letter>`,
 * which the terminal owns: `Ctrl+D` is EOF, `Ctrl+W` deletes a word, `Ctrl+[`
 * is Escape. An owner may rebind onto those anyway — it is their terminal — but
 * nothing here does it for them. An action missing from this table has no key
 * until the owner gives it one; it is still in the palette.
 */
export const DEFAULT_BINDINGS: Record<string, string> = {
  "command-palette": "Ctrl+Shift+P",
  "new-workspace": "Ctrl+Shift+N",
  "toggle-sidebar": "Ctrl+Shift+B",
  "next-workspace": "Ctrl+Shift+PageDown",
  "previous-workspace": "Ctrl+Shift+PageUp",
  "split-right": "Ctrl+Shift+D",
  "split-down": "Ctrl+Shift+E",
  "close-pane": "Ctrl+Shift+W",
  "toggle-zoom": "Ctrl+Shift+Enter",
  "cycle-layout": "Ctrl+Shift+Space",
  "focus-left": "Alt+ArrowLeft",
  "focus-right": "Alt+ArrowRight",
  "focus-up": "Alt+ArrowUp",
  "focus-down": "Alt+ArrowDown",
  "move-left": "Alt+Shift+ArrowLeft",
  "move-right": "Alt+Shift+ArrowRight",
  "move-up": "Alt+Shift+ArrowUp",
  "move-down": "Alt+Shift+ArrowDown",
  ...Object.fromEntries(
    [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => [`switch-workspace-${n}`, `Ctrl+${n}`]),
  ),
};

/** Bindings in canonical form, to the action each runs. */
export type Keymap = ReadonlyMap<string, AppAction>;

/** A keymap and what had to be ignored to build it. */
export interface BuiltKeymap {
  keymap: Keymap;
  problems: string[];
}

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

/** Modifiers in one fixed order, so a binding has exactly one spelling. */
function canonical(ctrl: boolean, alt: boolean, shift: boolean, key: string): string {
  const parts: string[] = [];
  if (ctrl) parts.push("Ctrl");
  if (alt) parts.push("Alt");
  if (shift) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

const ALIASES: Record<string, string> = {
  control: "Ctrl",
  ctrl: "Ctrl",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
  left: "ArrowLeft",
  right: "ArrowRight",
  up: "ArrowUp",
  down: "ArrowDown",
  esc: "Escape",
  return: "Enter",
  pagedown: "PageDown",
  pageup: "PageUp",
  space: "Space",
};

function alias(part: string): string {
  const known = ALIASES[part.toLowerCase()];
  if (known) return known;
  return part.length === 1 ? part.toUpperCase() : part;
}

/**
 * Rewrites a written binding into canonical form, or explains why it cannot be
 * used. A binding with no modifier is refused: it would take an ordinary
 * letter away from the terminal on every keystroke, which looks like the
 * keyboard is broken rather than like a setting.
 */
export function parseBinding(text: string): { binding: string } | { problem: string } {
  const parts = text
    .split("+")
    .map((part) => part.trim())
    .filter((part) => part.length > 0)
    .map(alias);
  const key = parts.at(-1);
  if (!key || parts.length === 0) {
    return { problem: `"${text}" is not a key combination` };
  }
  const modifiers = parts.slice(0, -1);
  const unknown = modifiers.find((part) => !["Ctrl", "Alt", "Shift"].includes(part));
  if (unknown) {
    return { problem: `"${text}" uses "${unknown}", which is not Ctrl, Alt, or Shift` };
  }
  if (modifiers.length === 0) {
    return {
      problem: `"${text}" has no modifier, so the terminal would never receive that key`,
    };
  }
  if (["Ctrl", "Alt", "Shift"].includes(key)) {
    return { problem: `"${text}" ends with a modifier, so there is no key to press` };
  }
  return {
    binding: canonical(
      modifiers.includes("Ctrl"),
      modifiers.includes("Alt"),
      modifiers.includes("Shift"),
      key,
    ),
  };
}

/**
 * The shipped bindings with the owner's `[keys]` table applied on top.
 *
 * An override that cannot be used is reported and skipped, leaving that action
 * on its default — never unbound, which would be a silent way to lose a
 * shortcut. A later binding wins a collision, so an override that lands on
 * another action's default takes it over rather than doing nothing.
 */
export function buildKeymap(overrides: Record<string, string> = {}): BuiltKeymap {
  const problems: string[] = [];
  const bindings = new Map<string, string>();

  for (const [action, written] of Object.entries(overrides)) {
    if (!(action in ACTIONS)) {
      problems.push(`keys.${action} is not something Dex can do`);
      continue;
    }
    const parsed = parseBinding(written);
    if ("problem" in parsed) {
      problems.push(`keys.${action}: ${parsed.problem}`);
      continue;
    }
    bindings.set(action, parsed.binding);
  }

  const keymap = new Map<string, AppAction>();
  for (const [action, shipped] of Object.entries(DEFAULT_BINDINGS)) {
    const rebound = bindings.get(action);
    if (rebound === undefined) {
      const parsed = parseBinding(shipped);
      if ("binding" in parsed) keymap.set(parsed.binding, ACTIONS[action]);
    }
  }
  // Overrides go in last so they win any collision with a default.
  for (const [action, binding] of bindings) {
    keymap.set(binding, ACTIONS[action]);
  }
  return { keymap, problems };
}

/** The bindings Dex ships with, for when the config has not been read yet. */
export const SHIPPED_KEYMAP: Keymap = buildKeymap().keymap;

/**
 * The keymap the other way round: each action's binding, for showing beside
 * its name. An action bound twice reports the first; one bound nowhere is
 * absent. Actions are compared by value, since the keymap holds copies.
 */
export function bindingsByAction(keymap: Keymap): Map<string, string> {
  const byValue = new Map<string, string>();
  for (const [name, action] of Object.entries(ACTIONS)) byValue.set(JSON.stringify(action), name);
  const result = new Map<string, string>();
  for (const [binding, action] of keymap) {
    const name = byValue.get(JSON.stringify(action));
    if (name !== undefined && !result.has(name)) result.set(name, binding);
  }
  return result;
}

/** The app action for a key event, or null if the terminal should get the key. */
export function appActionFor(
  event: KeyboardEvent,
  keymap: Keymap = SHIPPED_KEYMAP,
): AppAction | null {
  if (event.metaKey) return null;
  if (!event.ctrlKey && !event.altKey && !event.shiftKey) return null;
  const binding = canonical(event.ctrlKey, event.altKey, event.shiftKey, keyName(event));
  return keymap.get(binding) ?? null;
}
