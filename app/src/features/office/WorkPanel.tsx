import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { request } from "../../platform/daemon";
import { showError } from "../../platform/notices";
import { useActivity } from "../activity";
import { AgentStatusDot, STATUS_WORDS } from "../agents";
import { Avatar } from "./Avatar";
import { clockOutArgs, clockOutQuestion, memoArgs } from "./hire";
import type { Employee } from "./officeStore";
import { eventsOf, reportsTo } from "./panel";

/** How much of each the panel has room for. */
export const PANEL_SCREEN_LINES = 12;
const PANEL_EVENTS = 5;

interface Props {
  workspaceId: string;
  employee: Employee;
  staff: readonly Employee[];
  /** The last lines on their terminal. */
  screen: string[];
  /** Leaves the view for the terminals, on this pane. */
  onGoToPane: (paneId: string) => void;
  onClose: () => void;
}

/** One employee's work: what they were asked, what is on their screen, what they have done. */
export function WorkPanel({ workspaceId, employee, staff, screen, onGoToPane, onClose }: Props) {
  const { agent, persona, role } = employee;
  const events = eventsOf(useActivity(workspaceId)?.events, agent.id, PANEL_EVENTS);
  const [memo, setMemo] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  // Takes focus when it opens: a terminal that kept it would eat Escape.
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => panel.current?.focus(), [agent.id]);
  // And takes it back when the memo box goes: the button that was focused went
  // with it, and focus left on the page body would make Escape do nothing.
  const writing = memo !== null;
  useEffect(() => {
    if (!writing) panel.current?.focus();
  }, [writing]);

  const send = async () => {
    const args = memoArgs(workspaceId, agent.id, memo ?? "");
    if (!args) return;
    setSending(true);
    try {
      await request("context.message_send", args);
      setMemo(null);
    } catch (err) {
      showError(err);
    } finally {
      setSending(false);
    }
  };

  const [leaving, setLeaving] = useState(false);
  const clockOut = async () => {
    if (!(await ask(clockOutQuestion(persona.name, role), { title: "Clock out", kind: "warning" }))) return;
    setLeaving(true);
    try {
      // The panel closes by itself: it follows the agent, and the agent is about to go.
      await request("agent.stop", clockOutArgs(agent.id));
    } catch (err) {
      showError(err);
      setLeaving(false);
    }
  };

  return (
    <div
      ref={panel}
      className="office-panel"
      role="dialog"
      aria-label={`${persona.name}'s work`}
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === "Escape") onClose();
      }}
    >
      <div className="office-panel-head">
        <Avatar persona={persona} size={40} />
        <span className="office-panel-who">
          <span className="office-panel-name">{persona.name}</span>
          <span className="office-panel-role">
            {role} · reports to {reportsTo(employee, staff)}
            {agent.depth > 0 && ` · depth ${agent.depth}`}
          </span>
        </span>
        <button type="button" className="office-panel-close" aria-label="Close" onClick={onClose}>
          ✕
        </button>
      </div>
      <div className="office-panel-status">
        <AgentStatusDot status={agent.status} />
        {STATUS_WORDS[agent.status]}
        {agent.status_detail && ` · ${agent.status_detail}`}
      </div>
      <div className="office-panel-task">{agent.task_brief ?? "Started by you; no brief."}</div>

      <span className="office-panel-label">Their screen, live</span>
      <div className="office-panel-screen">
        {screen.length > 0 ? screen.map((line, i) => <div key={i}>{line}</div>) : <div className="quiet">nothing on screen</div>}
      </div>

      {events.length > 0 && (
        <>
          <span className="office-panel-label">Lately</span>
          <div className="office-panel-events">
            {events.map((event) => (
              <div key={event.seq}>
                <span>{event.kind}</span> {event.body}
              </div>
            ))}
          </div>
        </>
      )}

      {memo !== null && (
        <textarea
          className="office-panel-memo"
          autoFocus
          rows={3}
          placeholder={`A memo for ${persona.name}. It wakes them if they are idle.`}
          value={memo}
          onChange={(event) => setMemo(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) void send();
            // Escape backs out of the memo first, then the panel.
            if (event.key === "Escape") {
              event.stopPropagation();
              setMemo(null);
            }
          }}
        />
      )}
      <div className="office-panel-actions">
        {memo === null ? (
          <>
            <button
              type="button"
              className="primary"
              disabled={!agent.pane_id}
              onClick={() => agent.pane_id && onGoToPane(agent.pane_id)}
            >
              Go to pane
            </button>
            <button type="button" onClick={() => setMemo("")}>
              Send a memo
            </button>
            <button type="button" className="danger" disabled={leaving} onClick={() => void clockOut()}>
              {leaving ? "Leaving…" : "Clock out"}
            </button>
          </>
        ) : (
          <>
            <button type="button" className="primary" disabled={sending || !memo.trim()} onClick={() => void send()}>
              {sending ? "Sending…" : "Send"}
            </button>
            <button type="button" onClick={() => setMemo(null)}>
              Cancel
            </button>
          </>
        )}
      </div>
    </div>
  );
}
