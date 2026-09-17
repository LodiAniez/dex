import { useEffect, useRef, useState } from "react";
import { DexError, request } from "../../platform/daemon";
import type { WorkspaceView } from "../../platform/generated/WorkspaceView";
import { hireArgs, isHiringFreeze } from "./hire";

interface Props {
  workspace: WorkspaceView;
  onClose: () => void;
  /** The daemon refused for want of a seat. */
  onFreeze: () => void;
}

/**
 * HR's form: what the new agent is for, and optionally what to call its pane.
 * Sends `agent.spawn`, exactly as `dex agent spawn` does — limits, trust and
 * the welcome digest are the daemon's, and are not repeated here.
 */
export function HireDialog({ workspace, onClose, onFreeze }: Props) {
  const [task, setTask] = useState("");
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => field.current?.focus(), []);

  const args = hireArgs(workspace, task, label);
  const hire = async () => {
    if (!args || busy) return;
    setBusy(true);
    setProblem(null);
    try {
      await request("agent.spawn", args);
      onClose();
    } catch (err) {
      if (isHiringFreeze(err)) onFreeze();
      // The daemon's repair says what to do about it; the message says what happened.
      setProblem(err instanceof DexError ? `${err.message}. ${err.repair}` : String(err));
      setBusy(false);
    }
  };

  return (
    <div className="office-scrim" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="office-hire"
        aria-label="Hire an agent"
        onSubmit={(event) => {
          event.preventDefault();
          void hire();
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) void hire();
        }}
      >
        <div className="office-hire-title">Hire an agent</div>
        <label>
          What should they do?
          <textarea
            ref={field}
            rows={4}
            value={task}
            placeholder="Port the auth module to the new session API, then run the tests."
            onChange={(event) => setTask(event.target.value)}
          />
        </label>
        <label>
          Pane label <span className="optional">optional — other agents message them by it</span>
          <input value={label} placeholder="porter" onChange={(event) => setLabel(event.target.value)} />
        </label>
        {problem && <div className="office-hire-problem">{problem}</div>}
        <div className="office-hire-actions">
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={!args || busy}>
            {busy ? "Hiring…" : "Hire"}
          </button>
        </div>
      </form>
    </div>
  );
}
