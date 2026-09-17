import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { agentCounts, loadAgents, notifyTransitions, useAgents, watchAgentChanges, type PaneContext } from "../features/agents";
import { OfficeView, officeNameOf } from "../features/office";
import { CommandPalette, type PaletteItem } from "../features/palette";
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
import { currentKeymap, useUiSettings, watchConfig } from "../platform/config";
import { showError } from "../platform/notices";
import { focusTerminal, setShortcutFilter } from "../platform/terminalRegistry";
import { watchUpdates } from "../platform/update";
import { ACTIONS, appActionFor, type AppAction } from "./keybindings";
import { WorkspaceLayout } from "./LayoutView";
import { NoticeBar } from "./NoticeBar";
import { neighborPane } from "./paneGeometry";
import { type DoctorReport, fingerprint, shouldOffer } from "./setup";
import { Setup } from "./SetupPanel";
import { Sidebar } from "./Sidebar";
import { ViewSwitch } from "./ViewSwitch";
import { chooseMode, modeOfAction, rememberMode, showsPanes, storedMode, type ViewMode } from "./viewMode";
import { TitleBar } from "./TitleBar";

const SIDEBAR_PREF = "dex.sidebarOpen";
/** The set of setup problems the owner last chose to put off. */
const SETUP_DISMISSED = "dex.setupDismissed";

function readSidebarPref(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_PREF) !== "false";
  } catch {
    return true;
  }
}

function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writePref(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage unavailable: the panel will simply offer again next time.
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
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [setupReport, setSetupReport] = useState<DoctorReport | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);

  const agents = useAgents();

  // What the owner clicks wins over their config from then on; until they
  // click, the config decides, even if it changes while Dex is open.
  const settings = useUiSettings();
  const [chosen, setChosen] = useState<string | null>(storedMode);
  const mode = chooseMode(chosen, settings?.view);
  const overlay = useRef<HTMLDivElement>(null);
  const chooseView = (next: ViewMode) => {
    rememberMode(next);
    setChosen(next);
  };
  /** Back to the terminals, on this pane. */
  const goToPane = (paneId: string) => {
    chooseView("terminal");
    run(focusPane(paneId));
  };
  // The keyboard follows the view: a terminal hidden under the office must not
  // keep taking keystrokes, and coming back should land in the focused pane.
  useEffect(() => {
    if (showsPanes(mode)) {
      const pane = activePane();
      if (pane) focusTerminal(pane);
    } else {
      overlay.current?.focus();
    }
  }, [mode]);

  /** Runs `dex doctor`; opens the panel only when it should offer itself. */
  const checkSetup = async (offer: boolean) => {
    const report = await invoke<DoctorReport>("setup_check");
    setSetupReport(report);
    if (offer && shouldOffer(report, readPref(SETUP_DISMISSED))) setSetupOpen(true);
  };

  useEffect(() => {
    run(loadWorkspaces());
    run(loadAgents());
    watchConfig();
    watchUpdates();
    // First run: if hooks or the MCP server are missing, say so and offer to
    // fix it. A doctor that cannot run at all is logged, not shown — the app
    // is usable without it and the palette can ask again.
    void checkSetup(true).catch((err) => console.warn("setup check failed", err));
  }, []);

  // Toasts for agents that need attention in panes the user is not looking at.
  useEffect(() => watchAgentChanges((before, after) => notifyTransitions(before, after, locatePane, officeNameOf)), []);

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

  const perform = (action: AppAction) => {
    const pane = activePane();
    switch (action.kind) {
      case "command-palette":
        setPaletteOpen((open) => !open);
        return;
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
      case "open-diff":
        // The diff of whatever repository the focused pane is working in.
        setZoomed(null);
        if (pane) run(splitPane(pane, "right", "diff"));
        return;
      case "view-terminal":
      case "view-cards":
      case "view-office": {
        const next = modeOfAction(action.kind);
        if (next) chooseView(next);
        return;
      }
      case "open-setup":
        setSetupOpen(true);
        run(checkSetup(false));
        return;
    }
  };
  // The key handler below is installed once and must not go stale, so it
  // reaches `perform` through a ref that every render refreshes.
  const performRef = useRef(perform);
  performRef.current = perform;

  useEffect(() => {
    // Terminals hand app shortcuts on instead of sending them to the shell.
    // Both read the keymap when the key is pressed, never a captured copy, so a
    // config edit rebinds keys without this effect being torn down.
    setShortcutFilter((event) => appActionFor(event, currentKeymap()) !== null);
    const onKey = (event: KeyboardEvent) => {
      const action = appActionFor(event, currentKeymap());
      if (!action) return;
      event.preventDefault();
      performRef.current(action);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /** Closes the palette and puts the keyboard back where it was. */
  const closePalette = () => {
    setPaletteOpen(false);
    const pane = activePane();
    if (pane) focusTerminal(pane);
  };

  /** What choosing a palette row does. */
  const runItem = (item: PaletteItem) => {
    switch (item.kind) {
      case "workspace":
        return run(switchWorkspace(item.id));
      case "pane":
        run(switchWorkspace(item.workspaceId));
        return run(focusPane(item.id));
      case "command": {
        const action = ACTIONS[item.action];
        if (action) perform(action);
        return;
      }
    }
  };

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
        view={active ? { mode, onChoose: chooseView } : undefined}
      />
      <div className="app-body">
        <Sidebar
          open={sidebarOpen}
          creating={creating}
          onToggle={() => setSidebarOpen((open) => !open)}
          onCreatingChange={setCreating}
        />
        <main className="workspace-area">
          {/* Always mounted: a pane's shell starts when its terminal first attaches,
              so an agent hired from the office needs its pane to exist underneath. */}
          {active && <WorkspaceLayout workspace={active} zoomed={zoomedHere} />}
          {active && !showsPanes(mode) && (
            <div className="view-overlay" ref={overlay} tabIndex={-1}>
              <OfficeView
                workspaceId={active.id}
                view={mode === "office" ? "office" : "cards"}
                onGoToPane={goToPane}
                switcher={<ViewSwitch mode={mode} onChoose={chooseView} />}
              />
            </div>
          )}
        </main>
      </div>
      {paletteOpen && <CommandPalette onRun={runItem} onClose={closePalette} />}
      {setupOpen && setupReport && (
        <Setup
          report={setupReport}
          onRecheck={() => checkSetup(false)}
          onClose={() => {
            // Remember what was put off, so the same problems do not reopen
            // the panel every start — but new ones still do.
            writePref(SETUP_DISMISSED, fingerprint(setupReport));
            setSetupOpen(false);
            const pane = activePane();
            if (pane) focusTerminal(pane);
          }}
        />
      )}
      <NoticeBar />
    </div>
  );
}
