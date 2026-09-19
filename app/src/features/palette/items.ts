import type { WorkspaceList } from "../../platform/generated/WorkspaceList";
import { ACTIONS, type Keymap, bindingsByAction } from "../../shell/keybindings";
import { fuzzyMatch } from "./fuzzy";

/** Something the palette can run. */
export type PaletteItem =
  | { kind: "workspace"; id: string; title: string; detail: string; current: boolean }
  | { kind: "pane"; id: string; workspaceId: string; title: string; detail: string; current: boolean }
  | { kind: "command"; action: string; title: string; detail: string };

/** An item that matched, with where the query landed in its title. */
export interface RankedItem {
  item: PaletteItem;
  positions: number[];
}

/** What the palette searches, after any `w:` / `p:` prefix (PRD §13). */
export type Scope = "all" | "workspace" | "pane";

/** The query split into its scope and the text to match. */
export interface ParsedQuery {
  scope: Scope;
  needle: string;
}

const SCOPES: Record<string, Scope> = { "w:": "workspace", "p:": "pane" };

/** `w:api` searches workspaces for "api"; anything else searches everything. */
export function parseQuery(raw: string): ParsedQuery {
  const text = raw.trimStart();
  const prefix = text.slice(0, 2).toLowerCase();
  const scope = SCOPES[prefix];
  if (scope) return { scope, needle: text.slice(2).trim() };
  return { scope: "all", needle: text.trim() };
}

/**
 * What each command is called in the palette, by action name. Every action in
 * `ACTIONS` must appear here; a test holds that, since a missing one would
 * simply not be findable and nobody would notice.
 */
export const COMMAND_LABELS: Record<string, string> = {
  "command-palette": "Command palette",
  "new-workspace": "New workspace",
  "toggle-sidebar": "Toggle sidebar",
  "next-workspace": "Next workspace",
  "previous-workspace": "Previous workspace",
  "split-right": "Split pane right",
  "split-down": "Split pane down",
  "close-pane": "Close pane",
  "toggle-zoom": "Toggle pane zoom",
  "cycle-layout": "Cycle layout",
  "open-diff": "Show git diff",
  "open-activity": "Show activity",
  "view-terminal": "Terminal view",
  "view-office": "Office view",
  "cycle-view": "Next view",
  "open-setup": "Setup checks",
  "open-settings": "Settings",
  "focus-left": "Focus pane left",
  "focus-right": "Focus pane right",
  "focus-up": "Focus pane up",
  "focus-down": "Focus pane down",
  "move-left": "Move pane left",
  "move-right": "Move pane right",
  "move-up": "Move pane up",
  "move-down": "Move pane down",
  ...Object.fromEntries([1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => [`switch-workspace-${n}`, `Switch to workspace ${n}`])),
};

/** The last path segment, which is what people recognise a directory by. */
function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part.length > 0);
  return parts.at(-1) ?? path;
}

/**
 * Everything the palette can offer right now: workspaces, then their panes,
 * then commands. Commands come last because the other two are what changes
 * while you work, and an empty query shows the list in this order.
 */
export function buildItems(list: WorkspaceList | null, keymap: Keymap): PaletteItem[] {
  const items: PaletteItem[] = [];
  for (const ws of list?.workspaces ?? []) {
    items.push({
      kind: "workspace",
      id: ws.id,
      title: ws.name,
      detail: ws.root_path,
      current: ws.id === list?.active,
    });
  }
  for (const ws of list?.workspaces ?? []) {
    for (const pane of ws.panes) {
      items.push({
        kind: "pane",
        id: pane.id,
        workspaceId: ws.id,
        title: pane.label ?? baseName(pane.cwd),
        detail: `${ws.name} · ${pane.kind === "activity" ? "activity" : pane.cwd}`,
        current: ws.id === list?.active && ws.active_pane === pane.id,
      });
    }
  }
  const bindings = bindingsByAction(keymap);
  for (const action of Object.keys(ACTIONS)) {
    // The palette does not list itself: choosing it would do nothing visible.
    if (action === "command-palette") continue;
    items.push({
      kind: "command",
      action,
      title: COMMAND_LABELS[action] ?? action,
      detail: bindings.get(action) ?? "",
    });
  }
  return items;
}

function inScope(item: PaletteItem, scope: Scope): boolean {
  return scope === "all" || item.kind === scope;
}

/**
 * The items matching `raw`, best first. Ties keep build order, so with an
 * empty query the list reads workspaces, panes, commands, and equally good
 * matches do not jump around as the query grows.
 */
export function rankItems(items: PaletteItem[], raw: string): RankedItem[] {
  const { scope, needle } = parseQuery(raw);
  const ranked: { item: PaletteItem; positions: number[]; score: number; order: number }[] = [];
  items.forEach((item, order) => {
    if (!inScope(item, scope)) return;
    const match = fuzzyMatch(needle, item.title);
    if (match) ranked.push({ item, positions: match.positions, score: match.score, order });
  });
  ranked.sort((a, b) => b.score - a.score || a.order - b.order);
  return ranked.map(({ item, positions }) => ({ item, positions }));
}
