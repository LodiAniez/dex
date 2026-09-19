import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import "./palette.css";
import { currentKeymap, subscribeKeymap } from "../../platform/config";
import { useWorkspaces } from "../workspaces";
import { buildItems, parseQuery, rankItems, type PaletteItem, type RankedItem } from "./items";

/** The palette's title, with the matched letters marked. */
function Highlighted({ text, positions }: { text: string; positions: number[] }) {
  const marked = new Set(positions);
  return (
    <>
      {[...text].map((ch, i) =>
        marked.has(i) ? (
          <mark key={i}>{ch}</mark>
        ) : (
          <span key={i}>{ch}</span>
        ),
      )}
    </>
  );
}

const KIND_LABEL: Record<PaletteItem["kind"], string> = {
  workspace: "workspace",
  pane: "pane",
  command: "command",
};

/** How many rows to show; the rest is what typing more is for. */
const MAX_ROWS = 12;

export interface CommandPaletteProps {
  onRun: (item: PaletteItem) => void;
  onClose: () => void;
}

/**
 * `Ctrl+Shift+P`: fuzzy search over workspaces, panes, and commands, with
 * `w:` and `p:` scopes (PRD §13). Keyboard first — Up/Down, Enter, Escape —
 * and the mouse works too. Closing hands focus back to the caller.
 */
export function CommandPalette({ onRun, onClose }: CommandPaletteProps) {
  const list = useWorkspaces();
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const [keymapVersion, setKeymapVersion] = useState(0);

  // Bindings shown beside commands follow the config live.
  useEffect(() => subscribeKeymap(() => setKeymapVersion((v) => v + 1)), []);
  // `keymapVersion` stands in for the keymap itself, which lives outside React.
  const items = useMemo(() => buildItems(list, currentKeymap()), [list, keymapVersion]);
  const ranked: RankedItem[] = useMemo(() => rankItems(items, query).slice(0, MAX_ROWS), [items, query]);

  useEffect(() => input.current?.focus(), []);
  // A new query means a new list; start from the top of it.
  useEffect(() => setSelected(0), [query]);

  const run = (row: RankedItem | undefined) => {
    if (!row) return;
    onClose();
    onRun(row.item);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setSelected((i) => (ranked.length === 0 ? 0 : (i + 1) % ranked.length));
        return;
      case "ArrowUp":
        event.preventDefault();
        setSelected((i) => (ranked.length === 0 ? 0 : (i - 1 + ranked.length) % ranked.length));
        return;
      case "Enter":
        event.preventDefault();
        run(ranked[selected]);
        return;
      case "Escape":
        event.preventDefault();
        onClose();
        return;
    }
  };

  const { scope } = parseQuery(query);
  return (
    <div className="palette-backdrop" onMouseDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <input
          ref={input}
          className="palette-input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Search workspaces, panes, and commands — w: or p: to narrow"
          spellCheck={false}
          aria-activedescendant={ranked[selected] ? `palette-row-${selected}` : undefined}
        />
        <ul className="palette-list" role="listbox">
          {ranked.length === 0 && <li className="palette-empty">Nothing matches</li>}
          {ranked.map((row, i) => (
            <li
              key={rowKey(row.item)}
              id={`palette-row-${i}`}
              role="option"
              aria-selected={i === selected}
              className={`palette-row${i === selected ? " is-selected" : ""}`}
              onMouseEnter={() => setSelected(i)}
              onClick={() => run(row)}
            >
              <span className="palette-title">
                <Highlighted text={row.item.title} positions={row.positions} />
                {row.item.kind !== "command" && row.item.current && <span className="palette-current">current</span>}
              </span>
              <span className="palette-detail">{row.item.detail}</span>
              {scope === "all" && <span className="palette-kind">{KIND_LABEL[row.item.kind]}</span>}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function rowKey(item: PaletteItem): string {
  return item.kind === "command" ? `command:${item.action}` : `${item.kind}:${item.id}`;
}
