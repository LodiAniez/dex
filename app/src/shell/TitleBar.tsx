import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUpdate, useUpdate } from "../platform/update";
import { showError } from "../platform/notices";
import { ViewSwitch } from "./ViewSwitch";
import type { ViewMode } from "./viewMode";

interface Props {
  /** Active workspace name. */
  title?: string;
  /** Active workspace color. */
  color?: string | null;
  /** Agents in each state, across all workspaces (PRD §9.5). */
  counts: { running: number; waiting: number; error: number };
  /** Opens the workspace's activity stream; absent when no workspace is open. */
  onShowActivity?: () => void;
  /** How the workspace is being shown, and how to change it; absent when no workspace is open. */
  view?: { mode: ViewMode; onChoose: (mode: ViewMode) => void };
}

// Window decorations are off (PRD §13), so the app draws its own title bar.
// `data-tauri-drag-region` must be on the exact element under the cursor, so
// the title text carries it too; the buttons deliberately do not.
export function TitleBar({ title, color, counts, onShowActivity, view }: Props) {
  const win = getCurrentWindow();
  const update = useUpdate();
  return (
    <header className="titlebar" data-tauri-drag-region>
      <span className="titlebar-title" data-tauri-drag-region>
        {color && <span className="color-dot" style={{ background: color }} data-tauri-drag-region />}
        <span data-tauri-drag-region>{title ? `${title} — Dex` : "Dex"}</span>
      </span>
      <span className="agent-counts" data-tauri-drag-region>
        {counts.waiting > 0 && <span className="count waiting">{counts.waiting} need you</span>}
        {counts.error > 0 && <span className="count error">{counts.error} stopped</span>}
        {counts.running > 0 && <span className="count running">{counts.running} working</span>}
        {/* The counts say how many agents need something; this says what they
            are all doing, which is the question they prompt. */}
        {onShowActivity && (
          <button type="button" className="activity-open" title="Show workspace activity" onClick={onShowActivity}>
            activity
          </button>
        )}
        {/* The same workspace two ways: its panes, or its agents in an office. */}
        {view && <ViewSwitch mode={view.mode} onChoose={view.onChoose} />}
        {/* Only while a newer release exists; clicking opens its page. Nothing
            is downloaded or installed from here — the owner reads the notes
            and runs the installer themselves. */}
        {update && (
          <button
            type="button"
            className="update-pill"
            title={`Dex ${update.version} is available — open the release page`}
            onClick={() => void openUpdate(update).catch(showError)}
          >
            <span className="update-new">new</span> update {update.version}
          </button>
        )}
      </span>
      <div className="titlebar-buttons">
        {/* Glyphs from Segoe Fluent Icons, matching native Windows 11 caption buttons. */}
        <button type="button" aria-label="Minimize" onClick={() => void win.minimize()}>
          {""}
        </button>
        <button type="button" aria-label="Maximize" onClick={() => void win.toggleMaximize()}>
          {""}
        </button>
        <button type="button" aria-label="Close" className="close" onClick={() => void win.close()}>
          {""}
        </button>
      </div>
    </header>
  );
}
