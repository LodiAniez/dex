import { ask } from "@tauri-apps/plugin-dialog";
import { useRef, useState, type MouseEvent } from "react";
import type { WorkspaceView } from "../../platform/generated/WorkspaceView";
import { showError } from "../../platform/notices";
import { AgentStatusDot, useAgents, workspaceAttention } from "../agents";
import { ColorPicker } from "./ColorPicker";
import { ContextMenu } from "./ContextMenu";
import { deleteWorkspace, recolorWorkspace, renameWorkspace, switchWorkspace } from "./workspaceStore";
import { useShortcuts } from "../../platform/config";

interface Props {
  workspace: WorkspaceView;
  index: number;
  active: boolean;
  dragging: boolean;
  dropTarget: boolean;
  onDragStart: () => void;
  onDragOver: () => void;
  onDrop: () => void;
  onDragEnd: () => void;
}

type Point = { x: number; y: number };

function run(action: Promise<unknown>): void {
  void action.catch(showError);
}

/** One sidebar row: color dot, name, shortcut hint. Click switches; right-click opens the menu. */
export function WorkspaceItem(props: Props) {
  const { workspace, index, active } = props;
  const [renaming, setRenaming] = useState(false);
  const [picker, setPicker] = useState<Point | null>(null);
  const [menu, setMenu] = useState<Point | null>(null);
  const attention = workspaceAttention(useAgents(), workspace.id);
  const shortcut = useShortcuts().get(`switch-workspace-${index + 1}`);

  const openPicker = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    const rect = event.currentTarget.getBoundingClientRect();
    setPicker({ x: rect.left, y: rect.bottom + 6 });
  };

  const confirmDelete = async () => {
    const confirmed = await ask(`Delete "${workspace.name}"? Its terminals will be closed.`, {
      title: "Delete workspace",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    });
    if (confirmed) await deleteWorkspace(workspace.id);
  };

  const classes = ["workspace-item", active && "active", props.dragging && "dragging", props.dropTarget && "drop-target"];

  return (
    <li
      className={classes.filter(Boolean).join(" ")}
      title={workspace.root_path}
      draggable={!renaming}
      onDragStart={(event) => {
        event.dataTransfer.effectAllowed = "move";
        props.onDragStart();
      }}
      onDragOver={(event) => {
        event.preventDefault();
        props.onDragOver();
      }}
      onDrop={(event) => {
        event.preventDefault();
        props.onDrop();
      }}
      onDragEnd={props.onDragEnd}
      onClick={() => {
        if (!renaming) run(switchWorkspace(workspace.id));
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <button
        type="button"
        className={`color-dot${workspace.color ? "" : " empty"}`}
        style={workspace.color ? { background: workspace.color } : undefined}
        aria-label={`Change color of ${workspace.name}`}
        onClick={openPicker}
      />
      {renaming ? (
        <RenameInput
          initial={workspace.name}
          onDone={(name) => {
            setRenaming(false);
            if (name && name !== workspace.name) run(renameWorkspace(workspace.id, name));
          }}
        />
      ) : (
        <span className="workspace-name" onDoubleClick={() => setRenaming(true)}>
          {workspace.name}
        </span>
      )}
      {attention && <AgentStatusDot status={attention} />}
      {shortcut && <kbd className="shortcut">{shortcut}</kbd>}

      {picker && (
        <ColorPicker
          value={workspace.color}
          anchor={picker}
          onClose={() => setPicker(null)}
          onPick={(color) => {
            setPicker(null);
            run(recolorWorkspace(workspace.id, color));
          }}
        />
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            { label: "Rename", onSelect: () => setRenaming(true) },
            { label: "Change color", onSelect: () => setPicker(menu) },
            { label: "Delete", danger: true, onSelect: () => run(confirmDelete()) },
          ]}
        />
      )}
    </li>
  );
}

/** Inline name editor: Enter or blur saves, Escape cancels. */
function RenameInput({ initial, onDone }: { initial: string; onDone: (name: string | null) => void }) {
  const [value, setValue] = useState(initial);
  // Enter unmounts the input, which also fires blur; finish only once.
  const finished = useRef(false);
  const finish = (name: string | null) => {
    if (finished.current) return;
    finished.current = true;
    onDone(name);
  };
  return (
    <input
      className="rename-input"
      autoFocus
      value={value}
      maxLength={64}
      onChange={(event) => setValue(event.target.value)}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Enter") finish(value.trim());
        if (event.key === "Escape") finish(null);
      }}
      onBlur={() => finish(value.trim())}
    />
  );
}
