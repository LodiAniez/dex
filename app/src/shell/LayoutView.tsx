import { useEffect, useRef, useState, type PointerEvent } from "react";
import { ActivityPane } from "../features/activity";
import { AgentBadge, agentInPane, useAgents } from "../features/agents";
import { DiffPane, MarkdownPane, TerminalPane } from "../features/panes";
import { closePane, focusPane, setLayout } from "../features/workspaces";
import type { Layout } from "../platform/generated/Layout";
import type { PaneView } from "../platform/generated/PaneView";
import type { WorkspaceView } from "../platform/generated/WorkspaceView";
import { useShortcuts } from "../platform/config";
import { distroOf, runtimeLabel } from "../platform/runtimes";
import { showError } from "../platform/notices";
import { paneTitle, runsShell } from "./paneKind";
import { DropHint, PaneDragArea, usePaneDrag } from "./PaneDrag";
import { popOut, useDetached } from "./popouts";
import { canPopOut, visibleLayout, type Shown, type Side } from "./visibleLayout";
import { titled } from "./keybindings";

/** The workspace's pane tree, or just the zoomed pane. */
export function WorkspaceLayout({ workspace, zoomed }: { workspace: WorkspaceView; zoomed: string | null }) {
  const detached = useDetached();
  const zoomedPane = zoomed && !detached.has(zoomed) ? workspace.panes.find((pane) => pane.id === zoomed) : undefined;
  if (zoomedPane) return <PaneBox pane={zoomedPane} workspace={workspace} zoomed />;
  if (!workspace.layout) return null;
  // Panes in windows of their own are left out; the space closes up round them.
  const shown = visibleLayout(workspace.layout, detached);
  if (!shown) return null;
  return (
    <PaneDragArea>
      <Node layout={shown} workspace={workspace} />
    </PaneDragArea>
  );
}

interface NodeProps {
  layout: Shown;
  workspace: WorkspaceView;
}

function Node({ layout, workspace }: NodeProps) {
  if (layout.type === "leaf") {
    const pane = workspace.panes.find((p) => p.id === layout.pane_id);
    return pane ? <PaneBox pane={pane} workspace={workspace} zoomed={false} /> : null;
  }
  return <SplitNode layout={layout} workspace={workspace} />;
}

function SplitNode({ layout, workspace }: NodeProps & { layout: Extract<Shown, { type: "split" }> }) {
  // Where this split is in the stored tree, which may have more in it than is shown.
  const path = layout.path;
  const containerRef = useRef<HTMLDivElement>(null);
  // While dragging, the divider follows the pointer locally; one request commits on release.
  const [dragRatio, setDragRatio] = useState<number | null>(null);
  useEffect(() => setDragRatio(null), [layout.ratio]);
  const ratio = dragRatio ?? layout.ratio;
  const horizontal = layout.dir === "horizontal";

  const startDrag = (event: PointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container || !workspace.layout) return;
    event.preventDefault();
    const divider = event.currentTarget;
    divider.setPointerCapture(event.pointerId);
    const bounds = container.getBoundingClientRect();
    const root = workspace.layout;
    let latest = layout.ratio;

    const onMove = (move: globalThis.PointerEvent) => {
      const position = horizontal
        ? (move.clientX - bounds.left) / bounds.width
        : (move.clientY - bounds.top) / bounds.height;
      latest = Math.min(0.9, Math.max(0.1, position));
      setDragRatio(latest);
    };
    const onUp = () => {
      divider.removeEventListener("pointermove", onMove);
      divider.removeEventListener("pointerup", onUp);
      if (latest !== layout.ratio) {
        void setLayout(workspace.id, withRatio(root, path, latest)).catch(showError);
      }
    };
    divider.addEventListener("pointermove", onMove);
    divider.addEventListener("pointerup", onUp);
  };

  return (
    <div ref={containerRef} className={`split ${horizontal ? "row" : "column"}`}>
      <div className="split-child" style={{ flex: `${ratio} 1 0` }}>
        <Node layout={layout.a} workspace={workspace} />
      </div>
      <div
        className="divider"
        role="separator"
        aria-orientation={horizontal ? "vertical" : "horizontal"}
        onPointerDown={startDrag}
      />
      <div className="split-child" style={{ flex: `${1 - ratio} 1 0` }}>
        <Node layout={layout.b} workspace={workspace} />
      </div>
    </div>
  );
}

/** `layout` with the split at `path` set to `ratio`. */
function withRatio(layout: Layout, path: Side[], ratio: number): Layout {
  if (layout.type === "leaf") return layout;
  const [step, ...rest] = path;
  if (step === undefined) return { ...layout, ratio };
  return step === "a"
    ? { ...layout, a: withRatio(layout.a, rest, ratio) }
    : { ...layout, b: withRatio(layout.b, rest, ratio) };
}

function PaneBox({ pane, workspace, zoomed }: { pane: PaneView; workspace: WorkspaceView; zoomed: boolean }) {
  const shortcuts = useShortcuts();
  const active = workspace.active_pane === pane.id;
  const agent = agentInPane(useAgents(), pane.id);
  const { drag, begin } = usePaneDrag();
  const lifted = drag?.moving === pane.id && drag.side !== null;
  const hint = drag?.target === pane.id ? drag.side : null;
  const detached = useDetached();
  const poppable = !zoomed && workspace.layout !== null && canPopOut(workspace.layout, detached, pane.id);
  return (
    <div
      className={`pane${active ? " active" : ""}${lifted ? " lifted" : ""}`}
      data-pane-id={pane.id}
      onPointerDown={() => {
        if (!active) void focusPane(pane.id).catch(showError);
      }}
    >
      {/* Drag by the header to move the pane; a zoomed pane has nowhere to go. */}
      <div className="pane-header" title={zoomed ? undefined : "Drag to move this pane"} onPointerDown={zoomed ? undefined : (event) => begin(pane.id, event)}>
        {agent && <AgentBadge agent={agent} />}
        {pane.label && <span className="pane-label">{pane.label}</span>}
        <span className="pane-cwd" title={pane.cwd}>
          {paneTitle(pane)}
        </span>
        {distroOf(pane.runtime) && (
          <span className="pane-badge" title={`This pane's shell runs inside ${runtimeLabel(pane.runtime)}`}>
            {distroOf(pane.runtime)}
          </span>
        )}
        {zoomed && <span className="pane-badge">zoomed</span>}
        <button
          type="button"
          className="icon-button pane-popout"
          title={poppable ? "Open in its own window" : "The last pane in the window stays in it"}
          disabled={!poppable}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={() => void popOut(pane, workspace.id, pane.label ?? paneTitle(pane)).catch(showError)}
        >
          ↗
        </button>
        <button
          type="button"
          className="icon-button pane-close"
          title={titled("Close pane", shortcuts.get("close-pane"))}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={() => void closePane(pane.id).catch(showError)}
        >
          {""}
        </button>
      </div>
      {hint && <DropHint side={hint} />}
      {/* A non-terminal pane never mounts a terminal, so no shell is spawned. */}
      <PaneBody pane={pane} workspaceId={workspace.id} active={active} />
    </div>
  );
}

function PaneBody({ pane, workspaceId, active }: { pane: PaneView; workspaceId: string; active: boolean }) {
  switch (pane.kind) {
    case "activity":
      return <ActivityPane workspaceId={workspaceId} />;
    case "diff":
      return <DiffPane cwd={pane.cwd} />;
    case "markdown":
      return <MarkdownPane paneId={pane.id} />;
    default:
      return runsShell(pane.kind) ? (
        <TerminalPane paneId={pane.id} workspaceId={workspaceId} cwd={pane.cwd} runtime={pane.runtime} active={active} />
      ) : (
        <div className="pane-unknown">This version of Dex cannot show a “{pane.kind}” pane.</div>
      );
  }
}
