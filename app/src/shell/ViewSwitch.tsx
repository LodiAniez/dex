import { VIEW_MODES, type ViewMode } from "./viewMode";

/**
 * Terminal · Cards · Office. The same control sits in the title bar, where it
 * is always in reach, and in the pill at the foot of the cards and office
 * views, where the design put it.
 */
export function ViewSwitch({ mode, onChoose, compact }: { mode: ViewMode; onChoose: (mode: ViewMode) => void; compact?: boolean }) {
  return (
    <span className={`view-switch${compact ? " compact" : ""}`} role="group" aria-label="View">
      {VIEW_MODES.map(({ id, label, hint }) => (
        <button key={id} type="button" title={hint} aria-pressed={mode === id} onClick={() => onChoose(id)}>
          {label}
        </button>
      ))}
    </span>
  );
}
