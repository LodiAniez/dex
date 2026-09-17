import { VIEW_MODES, type ViewMode } from "./viewMode";

/**
 * Terminal · Office, in the title bar: one switch, in reach from either
 * view. The design had a second at the foot of the office; with this one
 * always on screen it only said the same thing twice.
 */
export function ViewSwitch({ mode, onChoose }: { mode: ViewMode; onChoose: (mode: ViewMode) => void }) {
  return (
    <span className="view-switch" role="group" aria-label="View">
      {VIEW_MODES.map(({ id, label, hint }) => (
        <button key={id} type="button" title={hint} aria-pressed={mode === id} onClick={() => onChoose(id)}>
          {label}
        </button>
      ))}
    </span>
  );
}
