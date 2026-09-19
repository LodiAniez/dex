import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUpdate, useUpdate } from "../platform/update";
import { showError } from "../platform/notices";
import { IS_MAC } from "./keybindings";
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
  /** Opens Dex's settings: the terminal it opens. */
  onOpenSettings: () => void;
}

// Window decorations are off on Windows (PRD §13), so the app draws its own
// title bar and caption buttons. On macOS the window keeps its own traffic
// lights, drawn over this bar (`tauri.macos.conf.json`), so the bar leaves
// room for them and draws no buttons of its own.
// `data-tauri-drag-region` must be on the exact element under the cursor, so
// the title text carries it too; the buttons deliberately do not.
export function TitleBar({ title, color, counts, onShowActivity, view, onOpenSettings }: Props) {
  const win = getCurrentWindow();
  const update = useUpdate();
  const fullscreen = useMacFullscreen();
  return (
    <header className={["titlebar", IS_MAC && "mac", fullscreen && "fullscreen"].filter(Boolean).join(" ")} data-tauri-drag-region>
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
        <button type="button" className="settings-open" title="Settings" aria-label="Settings" onClick={onOpenSettings}>
          <GearIcon />
        </button>
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
      {!IS_MAC && (
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
      )}
    </header>
  );
}

/** A gear, drawn here: the caption glyphs' font is Windows-only. */
function GearIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

/**
 * Whether a Mac window is fullscreen, where macOS hides the traffic lights and
 * the bar needs no room for them. Always false elsewhere.
 */
function useMacFullscreen(): boolean {
  const [fullscreen, setFullscreen] = useState(false);
  useEffect(() => {
    if (!IS_MAC) return;
    const win = getCurrentWindow();
    const check = () => void win.isFullscreen().then(setFullscreen, () => {});
    check();
    const unlisten = win.onResized(check);
    return () => void unlisten.then((stop) => stop());
  }, []);
  return fullscreen;
}
