import { useState } from "react";
import { showError } from "../../platform/notices";
import { WorkspaceItem } from "./WorkspaceItem";
import { reorderWorkspaces, switchWorkspace, useWorkspaces } from "./workspaceStore";
import { forPlatform } from "../../shell/keybindings";

/** The sidebar's workspace rows. Drag a row to reorder. */
export function WorkspaceList() {
  const list = useWorkspaces();
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  if (!list) return null;

  const ids = list.workspaces.map((ws) => ws.id);
  const drop = () => {
    if (dragId && overId && dragId !== overId) {
      const movingDown = ids.indexOf(dragId) < ids.indexOf(overId);
      const order = ids.filter((id) => id !== dragId);
      order.splice(order.indexOf(overId) + (movingDown ? 1 : 0), 0, dragId);
      void reorderWorkspaces(order).catch(showError);
    }
    setDragId(null);
    setOverId(null);
  };

  return (
    <ul className="workspace-list">
      {list.workspaces.map((workspace, index) => (
        <WorkspaceItem
          key={workspace.id}
          workspace={workspace}
          index={index}
          active={workspace.id === list.active}
          dragging={workspace.id === dragId}
          dropTarget={workspace.id === overId && overId !== dragId}
          onDragStart={() => setDragId(workspace.id)}
          onDragOver={() => setOverId(workspace.id)}
          onDrop={drop}
          onDragEnd={() => {
            setDragId(null);
            setOverId(null);
          }}
        />
      ))}
    </ul>
  );
}

/** The collapsed sidebar: one color dot per workspace. */
export function WorkspaceDots() {
  const list = useWorkspaces();
  if (!list) return null;
  return (
    <div className="workspace-dots">
      {list.workspaces.map((workspace, index) => (
        <button
          key={workspace.id}
          type="button"
          className={`dot-button${workspace.id === list.active ? " active" : ""}`}
          title={index < 9 ? `${workspace.name} (${forPlatform(`Ctrl+${index + 1}`)})` : workspace.name}
          onClick={() => void switchWorkspace(workspace.id).catch(showError)}
        >
          <span
            className={`color-dot${workspace.color ? "" : " empty"}`}
            style={workspace.color ? { background: workspace.color } : undefined}
          />
        </button>
      ))}
    </div>
  );
}
