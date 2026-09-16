import { getCurrentWindow } from "@tauri-apps/api/window";

interface Props {
  /** Active workspace name. */
  title?: string;
  /** Active workspace color. */
  color?: string | null;
  /** Agents in each state, across all workspaces (PRD §9.5). */
  counts: { running: number; waiting: number; error: number };
}

// Window decorations are off (PRD §13), so the app draws its own title bar.
// `data-tauri-drag-region` must be on the exact element under the cursor, so
// the title text carries it too; the buttons deliberately do not.
export function TitleBar({ title, color, counts }: Props) {
  const win = getCurrentWindow();
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
