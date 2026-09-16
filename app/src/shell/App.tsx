import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { agentCounts, loadAgents, notifyTransitions, useAgents, watchAgentChanges, type PaneContext } from "../features/agents";
import {
  closePane,
  cycleLayout,
  focusPane,
  getWorkspaces,
  loadWorkspaces,
  showActivity,
  splitPane,
  swapPanes,
  switchWorkspace,
  useWorkspaces,
} from "../features/workspaces";
import type { WorkspaceView } from "../platform/generated/WorkspaceView";
import { currentKeymap, watchConfig } from "../platform/config";
import { showError } from "../platform/notices";
import { setShortcutFilter } from "../platform/terminalRegistry";
import { appActionFor, type AppAction } from "./keybindings";
import { WorkspaceLayout } from "./LayoutView";
import { NoticeBar } from "./NoticeBar";
import { neighborPane } from "./paneGeometry";
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

function activeWorkspace(): WorkspaceView | undefined {
  const list = getWorkspaces();
  return list?.workspaces.find((ws) => ws.id === list.active);
}

/** The focused pane of the active workspace (its first pane if none is recorded). */
function activePane(): string | undefined {
  const ws = activeWorkspace();
  return ws?.active_pane ?? ws?.panes[0]?.id;
}

function run(action: Promise<unknown> | undefined): void {
  void action?.catch(showError);
}

/** Where a pane is, and whether the user is looking at it (for notifications). */
function locatePane(paneId: string): PaneContext | null {
  const list = getWorkspaces();
  const workspace = list?.workspaces.find((ws) => ws.panes.some((pane) => pane.id === paneId));
  const pane = workspace?.panes.find((p) => p.id === paneId);
  if (!list || !workspace || !pane) return null;
  const looking = document.hasFocus() && list.active === workspace.id && workspace.active_pane === paneId;
  return { workspaceId: workspace.id, workspaceName: workspace.name, paneName: pane.label ?? "pane", looking };
}

/** Switches to the workspace `offset` places from the active one, wrapping around. */
function switchBy(offset: number): void {
  const list = getWorkspaces();
  if (!list || list.workspaces.length === 0) return;
  const count = list.workspaces.length;
  const current = Math.max(0, list.workspaces.findIndex((ws) => ws.id === list.active));
  run(switchWorkspace(list.workspaces[(current + offset + count) % count].id));
}

/** Window root: title bar, sidebar, and the active workspace's panes. */
export function App() {
  const list = useWorkspaces();
  const [sidebarOpen, setSidebarOpen] = useState(readSidebarPref);
  const [creating, setCreating] = useState(false);
  const [zoomed, setZoomed] = useState<string | null>(null);

  const agents = useAgents();

  useEffect(() => {
    run(loadWorkspaces());
    run(loadAgents());
    watchConfig();
  }, []);

  // Toasts for agents that need attention in panes the user is not looking at.
  useEffect(() => watchAgentChanges((before, after) => notifyTransitions(before, after, locatePane)), []);

  // A clicked toast brings the app forward; show the pane it was about.
  useEffect(() => {
    const unlisten = listen<{ workspace: string; pane: string }>("dex://focus-pane", (event) => {
      run(switchWorkspace(event.payload.workspace));
      run(focusPane(event.payload.pane));
    });
    return () => void unlisten.then((stop) => stop());
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
      const pane = activePane();
      switch (action.kind) {
        case "new-workspace":
          setSidebarOpen(true);
          setCreating(true);
          return;
        case "toggle-sidebar":
          setSidebarOpen((open) => !open);
          return;
        case "next-workspace":
          return switchBy(1);
        case "previous-workspace":
          return switchBy(-1);
        case "switch-workspace": {
          const target = getWorkspaces()?.workspaces[action.index];
          if (target) run(switchWorkspace(target.id));
          return;
        }
        case "split-pane":
          setZoomed(null);
          if (pane) run(splitPane(pane, action.direction));
          return;
        case "close-pane":
          setZoomed(null);
          if (pane) run(closePane(pane));
          return;
        case "focus-pane": {
          const next = pane && neighborPane(pane, action.direction);
          if (next) run(focusPane(next));
          return;
        }
        case "move-pane": {
          const next = pane && neighborPane(pane, action.direction);
          if (pane && next) run(swapPanes(pane, next));
          return;
        }
        case "toggle-zoom":
          setZoomed((current) => (current ? null : (pane ?? null)));
          return;
        case "cycle-layout": {
          setZoomed(null);
          const ws = activeWorkspace();
          if (ws) run(cycleLayout(ws.id));
          return;
        }
      }
    };
    // Terminals hand app shortcuts on instead of sending them to the shell.
    // Both read the keymap when the key is pressed, never a captured copy, so a
    // config edit rebinds keys without this effect being torn down.
    setShortcutFilter((event) => appActionFor(event, currentKeymap()) !== null);
    const onKey = (event: KeyboardEvent) => {
      const action = appActionFor(event, currentKeymap());
      if (!action) return;
      event.preventDefault();
      perform(action);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const active = list?.workspaces.find((ws) => ws.id === list.active);
  // Zoom belongs to one pane of the active workspace; anywhere else it is off.
  const zoomedHere = active?.panes.some((pane) => pane.id === zoomed) ? zoomed : null;

  return (
    <div className="app">
      <TitleBar
        title={active?.name}
        color={active?.color}
        counts={agentCounts(agents)}
        onShowActivity={active ? () => run(showActivity(active)) : undefined}
      />
      <div className="app-body">
        <Sidebar
          open={sidebarOpen}
          creating={creating}
          onToggle={() => setSidebarOpen((open) => !open)}
          onCreatingChange={setCreating}
        />
        <main className="workspace-area">{active && <WorkspaceLayout workspace={active} zoomed={zoomedHere} />}</main>
      </div>
      <NoticeBar />
    </div>
  );
}
