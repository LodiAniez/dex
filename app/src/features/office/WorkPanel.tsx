import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { request } from "../../platform/daemon";
import { showError } from "../../platform/notices";
import { useActivity } from "../activity";
import { AgentStatusDot, STATUS_WORDS } from "../agents";
import { Avatar } from "./Avatar";
import { clockOutArgs, clockOutQuestion, memoArgs } from "./hire";
import type { Employee } from "./officeStore";
import { ago, doneBy, reportsTo } from "./panel";
import { phrase, type Who } from "./phrasing";
import { promptArgs, whyNoPrompt } from "./prompt";

/** How much of each the panel has room for. */
export const PANEL_SCREEN_LINES = 12;

interface Props {
  workspaceId: string;
  employee: Employee;
  staff: readonly Employee[];
  /** Who is who, for retelling what they did by name. */
  who: Who;
  /** The last lines on their terminal. */
  screen: string[];
  /** Leaves the view for the terminals, on this pane. */
  onGoToPane: (paneId: string) => void;
  onClose: () => void;
}

/** One employee's work: what they were asked, what is on their screen, what they have done. */
export function WorkPanel({ workspaceId, employee, staff, who, screen, onGoToPane, onClose }: Props) {
  const { agent, persona, role } = employee;
  const done = doneBy(useActivity(workspaceId)?.events, agent.id);
  // Re-rendered by the screen poll every second, which keeps the times honest.
  const now = Date.now();
  // What is being written, if anything: a memo for their inbox, or a prompt
  // typed into their terminal as the owner's turn.
  const [writing, setWriting] = useState<"memo" | "prompt" | null>(null);
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const noPrompt = whyNoPrompt(agent);
  const open = (kind: "memo" | "prompt") => {
    setText("");
    setWriting(kind);
  };

  // Takes focus when it opens: a terminal that kept it would eat Escape.
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => panel.current?.focus(), [agent.id]);
  // And takes it back when the box goes: the button that was focused went
  // with it, and focus left on the page body would make Escape do nothing.
  const composing = writing !== null;
  useEffect(() => {
    if (!composing) panel.current?.focus();
  }, [composing]);

  const send = async () => {
    const call =
      writing === "prompt"
        ? { cmd: "agent.prompt", args: promptArgs(agent.id, text) }
        : { cmd: "context.message_send", args: memoArgs(workspaceId, agent.id, text) };
    if (!call || !call.args) return;
    setSending(true);
    try {
      await request(call.cmd, call.args);
      setWriting(null);
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

      <span className="office-panel-label">What they have done</span>
      <div className="office-panel-done">
        {done.length > 0 ? (
          done.map((event) => {
            const line = phrase(event, who);
            return (
              <div key={event.seq} className={`office-panel-did ${line.tone}`}>
                <span className="when">{ago(event.created_at, now)}</span>
                <span className="what">{line.who === persona.name ? line.text : `${line.who} ${line.text}`}</span>
              </div>
            );
          })
        ) : (
          <div className="quiet">Nothing on record yet. Notes, messages and what they store show up here.</div>
        )}
      </div>

      {writing !== null && (
        <textarea
          className="office-panel-memo"
          autoFocus
          rows={3}
          placeholder={
            writing === "prompt"
              ? `A prompt for ${persona.name}, typed into their terminal as your turn.`
              : `A memo for ${persona.name}'s inbox. It wakes them if they are idle.`
          }
          value={text}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) void send();
            // Escape backs out of the box first, then the panel.
            if (event.key === "Escape") {
              event.stopPropagation();
              setWriting(null);
            }
          }}
        />
      )}
      <div className="office-panel-actions">
        {writing === null ? (
          <>
            <button
              type="button"
              className="primary"
              disabled={!agent.pane_id}
              onClick={() => agent.pane_id && onGoToPane(agent.pane_id)}
            >
              Go to pane
            </button>
            <button
              type="button"
              disabled={noPrompt !== null}
              title={noPrompt ? `Cannot prompt them: ${noPrompt}. Go to their pane instead.` : "Type into their terminal, as your turn"}
              onClick={() => open("prompt")}
            >
              Prompt
            </button>
            <button type="button" title="Leave a message in their inbox" onClick={() => open("memo")}>
              Memo
            </button>
            <button type="button" className="danger" disabled={leaving} onClick={() => void clockOut()}>
              {leaving ? "Leaving…" : "Clock out"}
            </button>
          </>
        ) : (
          <>
            <button type="button" className="primary" disabled={sending || !text.trim()} onClick={() => void send()}>
              {sending ? "Sending…" : writing === "prompt" ? "Send prompt" : "Send memo"}
            </button>
            <button type="button" onClick={() => setWriting(null)}>
              Cancel
            </button>
          </>
        )}
      </div>
    </div>
  );
}
