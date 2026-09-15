import { useEffect, useState } from "react";
import { getWorkspaces, loadWorkspaces, switchWorkspace, useWorkspaces } from "../features/workspaces";
import { showError } from "../platform/notices";
import { setShortcutFilter } from "../platform/terminalRegistry";
import { appActionFor, type AppAction } from "./keybindings";
import { LayoutView } from "./LayoutView";
import { NoticeBar } from "./NoticeBar";
import { Sidebar } from "./Sidebar";
import { TitleBar } from "./TitleBar";

const SIDEBAR_PREF = "dex.sidebarOpen";

function readSidebarPref(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_PREF) !== "false";
  } catch {
    return true;
  }
}

/** Switches to the workspace `offset` places from the active one, wrapping around. */
function switchBy(offset: number): void {
  const list = getWorkspaces();
  if (!list || list.workspaces.length === 0) return;
  const count = list.workspaces.length;
  const current = Math.max(0, list.workspaces.findIndex((ws) => ws.id === list.active));
  void switchWorkspace(list.workspaces[(current + offset + count) % count].id).catch(showError);
}

/** Window root: title bar, sidebar, and the active workspace's panes. */
export function App() {
  const list = useWorkspaces();
  const [sidebarOpen, setSidebarOpen] = useState(readSidebarPref);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    void loadWorkspaces().catch(showError);
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(SIDEBAR_PREF, String(sidebarOpen));
    } catch {
      // Storage unavailable: the sidebar simply opens by default next time.
    }
  }, [sidebarOpen]);

  useEffect(() => {
    const perform = (action: AppAction) => {
      switch (action.kind) {
        case "new-workspace":
          setSidebarOpen(true);
          setCreating(true);
          return;
        case "toggle-sidebar":
          setSidebarOpen((open) => !open);
          return;
        case "next-workspace":
          switchBy(1);
          return;
        case "previous-workspace":
          switchBy(-1);
          return;
        case "switch-workspace": {
          const target = getWorkspaces()?.workspaces[action.index];
          if (target) void switchWorkspace(target.id).catch(showError);
          return;
        }
      }
    };
    // Terminals hand app shortcuts on instead of sending them to the shell.
    setShortcutFilter((event) => appActionFor(event) !== null);
    const onKey = (event: KeyboardEvent) => {
      const action = appActionFor(event);
      if (!action) return;
      event.preventDefault();
      perform(action);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const active = list?.workspaces.find((ws) => ws.id === list.active);

  return (
    <div className="app">
      <TitleBar title={active?.name} color={active?.color} />
      <div className="app-body">
        <Sidebar
          open={sidebarOpen}
          creating={creating}
          onToggle={() => setSidebarOpen((open) => !open)}
          onCreatingChange={setCreating}
        />
        <main className="workspace-area">
          {active?.layout && <LayoutView layout={active.layout} workspace={active} />}
        </main>
      </div>
      <NoticeBar />
    </div>
  );
}
