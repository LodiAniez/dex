import { NewWorkspaceForm, WorkspaceDots, WorkspaceList } from "../features/workspaces";

interface Props {
  open: boolean;
  creating: boolean;
  onToggle: () => void;
  onCreatingChange: (creating: boolean) => void;
}

// Segoe Fluent Icons glyphs.
const ICON_ADD = "";
const ICON_EXPAND = "";
const ICON_COLLAPSE = "";

/** Collapsible workspace sidebar (PRD §13). Collapsed, it shows one color dot per workspace. */
export function Sidebar({ open, creating, onToggle, onCreatingChange }: Props) {
  if (!open) {
    return (
      <nav className="sidebar collapsed" aria-label="Workspaces">
        <button type="button" className="icon-button" title="Show sidebar (Ctrl+Shift+B)" onClick={onToggle}>
          {ICON_EXPAND}
        </button>
        <WorkspaceDots />
        <button
          type="button"
          className="icon-button"
          title="New workspace (Ctrl+Shift+N)"
          onClick={() => {
            onToggle();
            onCreatingChange(true);
          }}
        >
          {ICON_ADD}
        </button>
      </nav>
    );
  }

  return (
    <nav className="sidebar" aria-label="Workspaces">
      <div className="sidebar-header">
        <span>Workspaces</span>
        <div>
          <button
            type="button"
            className="icon-button"
            title="New workspace (Ctrl+Shift+N)"
            onClick={() => onCreatingChange(!creating)}
          >
            {ICON_ADD}
          </button>
          <button type="button" className="icon-button" title="Hide sidebar (Ctrl+Shift+B)" onClick={onToggle}>
            {ICON_COLLAPSE}
          </button>
        </div>
      </div>
      {creating && <NewWorkspaceForm onDone={() => onCreatingChange(false)} />}
      <WorkspaceList />
    </nav>
  );
}
