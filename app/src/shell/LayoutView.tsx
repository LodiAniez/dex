import { useEffect, useRef, useState, type PointerEvent } from "react";
import { ActivityPane } from "../features/activity";
import { AgentBadge, agentInPane, useAgents } from "../features/agents";
import { DiffPane, MarkdownPane, TerminalPane } from "../features/panes";
import { closePane, focusPane, setLayout } from "../features/workspaces";
import type { Layout } from "../platform/generated/Layout";
import type { PaneView } from "../platform/generated/PaneView";
import type { WorkspaceView } from "../platform/generated/WorkspaceView";
import { showError } from "../platform/notices";
import { paneTitle, runsShell } from "./paneKind";

type Side = "a" | "b";

/** The workspace's pane tree, or just the zoomed pane. */
export function WorkspaceLayout({ workspace, zoomed }: { workspace: WorkspaceView; zoomed: string | null }) {
  const zoomedPane = zoomed ? workspace.panes.find((pane) => pane.id === zoomed) : undefined;
  if (zoomedPane) return <PaneBox pane={zoomedPane} workspace={workspace} zoomed />;
  if (!workspace.layout) return null;
  return <Node layout={workspace.layout} path={[]} workspace={workspace} />;
}

interface NodeProps {
  layout: Layout;
  path: Side[];
  workspace: WorkspaceView;
}

function Node({ layout, path, workspace }: NodeProps) {
  if (layout.type === "leaf") {
    const pane = workspace.panes.find((p) => p.id === layout.pane_id);
    return pane ? <PaneBox pane={pane} workspace={workspace} zoomed={false} /> : null;
  }
  return <SplitNode layout={layout} path={path} workspace={workspace} />;
}

function SplitNode({ layout, path, workspace }: NodeProps & { layout: Extract<Layout, { type: "split" }> }) {
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
        <Node layout={layout.a} path={[...path, "a"]} workspace={workspace} />
      </div>
      <div
        className="divider"
        role="separator"
        aria-orientation={horizontal ? "vertical" : "horizontal"}
        onPointerDown={startDrag}
      />
      <div className="split-child" style={{ flex: `${1 - ratio} 1 0` }}>
        <Node layout={layout.b} path={[...path, "b"]} workspace={workspace} />
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
  const active = workspace.active_pane === pane.id;
  const agent = agentInPane(useAgents(), pane.id);
  return (
    <div
      className={`pane${active ? " active" : ""}`}
      data-pane-id={pane.id}
      onPointerDown={() => {
        if (!active) void focusPane(pane.id).catch(showError);
      }}
    >
      <div className="pane-header">
        {agent && <AgentBadge agent={agent} />}
        {pane.label && <span className="pane-label">{pane.label}</span>}
        <span className="pane-cwd" title={pane.cwd}>
          {paneTitle(pane)}
        </span>
        {zoomed && <span className="pane-badge">zoomed</span>}
        <button
          type="button"
          className="icon-button pane-close"
          title="Close pane (Ctrl+Shift+W)"
          onPointerDown={(event) => event.stopPropagation()}
          onClick={() => void closePane(pane.id).catch(showError)}
        >
          {""}
        </button>
      </div>
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
        <TerminalPane paneId={pane.id} workspaceId={workspaceId} cwd={pane.cwd} active={active} />
      ) : (
        <div className="pane-unknown">This version of Dex cannot show a “{pane.kind}” pane.</div>
      );
  }
}
