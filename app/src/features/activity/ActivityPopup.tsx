import { useEffect, useRef } from "react";
import { ActivityPane } from "./ActivityPane";

/**
 * The workspace's activity as a dialog over whatever view is up. It used to
 * open as a pane, which meant leaving the cards or the office to read it; a
 * pane of it can still be made from the CLI for anyone who wants it docked.
 */
export function ActivityPopup({ workspaceId, workspaceName, onClose }: { workspaceId: string; workspaceName: string; onClose: () => void }) {
  // Takes focus when it opens: a terminal that kept it would eat Escape.
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => box.current?.focus(), []);
  return (
    <div className="activity-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div
        ref={box}
        className="activity-popup"
        role="dialog"
        aria-label={`Activity in ${workspaceName}`}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      >
        <div className="activity-popup-head">
          <span>Activity · {workspaceName}</span>
          <button type="button" aria-label="Close" onClick={onClose}>
            ✕
          </button>
        </div>
        <ActivityPane workspaceId={workspaceId} />
      </div>
    </div>
  );
}
