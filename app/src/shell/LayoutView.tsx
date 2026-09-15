import { TerminalPane } from "../features/panes/TerminalPane";
import type { Layout } from "../platform/generated/Layout";
import type { WorkspaceView } from "../platform/generated/WorkspaceView";

/** Renders a workspace's pane tree. Splits are created from M2; rendering them is already here. */
export function LayoutView({ layout, workspace }: { layout: Layout; workspace: WorkspaceView }) {
  if (layout.type === "leaf") {
    const pane = workspace.panes.find((p) => p.id === layout.pane_id);
    return pane ? <TerminalPane key={pane.id} paneId={pane.id} cwd={pane.cwd} /> : null;
  }
  return (
    <div className="split" style={{ flexDirection: layout.dir === "horizontal" ? "row" : "column" }}>
      <div className="split-child" style={{ flex: layout.ratio }}>
        <LayoutView layout={layout.a} workspace={workspace} />
      </div>
      <div className="split-child" style={{ flex: 1 - layout.ratio }}>
        <LayoutView layout={layout.b} workspace={workspace} />
      </div>
    </div>
  );
}
