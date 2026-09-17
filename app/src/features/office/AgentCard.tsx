import { useRef, useState } from "react";
import { AgentStatusDot, STATUS_WORDS } from "../agents";
import { useAgentActions } from "./agentActions";
import { Avatar } from "./Avatar";
import type { Employee } from "./officeStore";
import { needsYou } from "./panel";
import { boxState, sendsOn, whyNoPrompt } from "./prompt";

interface Props {
  employee: Employee;
  /** What is on their terminal right now, oldest line first. */
  screen: string[];
  onGoToPane: (paneId: string) => void;
  /** Opens their panel: who they report to, what they have done, a memo. */
  onDetails: (employee: Employee) => void;
  /** Opens their output large, with its history. */
  onExpand: (employee: Employee) => void;
}

/**
 * One agent, as their screen. The card *is* what they are printing, live, with
 * who they are above it and what the owner can do below it: type at them, go to
 * their pane, see more, clock them out. Nothing opens when it is clicked - the
 * card is already the thing worth looking at.
 */
export function AgentCard({ employee, screen, onGoToPane, onDetails, onExpand }: Props) {
  const { agent, persona, role } = employee;
  const { prompt, sending, clockOut, leaving } = useAgentActions(employee);
  const [text, setText] = useState("");
  const attention = needsYou(agent.status, agent.status_detail);
  const noPrompt = whyNoPrompt(agent);

  const box = useRef<HTMLTextAreaElement>(null);

  const send = async () => {
    if (await prompt(text)) setText("");
    // Sent or refused, the keyboard stays with the box: the next thing typed is
    // the next thing to tell them.
    box.current?.focus();
  };

  return (
    <div className={`office-card ${agent.status}`}>
      <div className="office-card-head">
        <Avatar persona={persona} size={30} />
        <span className="office-card-who">
          <span className="office-card-name">{persona.name}</span>
          <span className="office-card-role" title={agent.task_brief ?? undefined}>
            {role}
          </span>
        </span>
        <span className="office-card-status">
          <AgentStatusDot status={agent.status} />
          {STATUS_WORDS[agent.status]}
        </span>
      </div>

      {/* What they were asked to do, in a line; the whole of it is in Details. */}
      {agent.task_brief && (
        <div className="office-card-brief" title={agent.task_brief}>
          {agent.task_brief}
        </div>
      )}

      {attention && (
        <button type="button" className={`office-card-needs ${agent.status}`} disabled={!agent.pane_id} onClick={() => agent.pane_id && onGoToPane(agent.pane_id)}>
          {attention} <span className="go">Go to pane →</span>
        </button>
      )}

      {/* Their terminal, as text. Anchored to the bottom, like a terminal: the newest line is what matters. */}
      <div className="office-card-screen" aria-label={`${persona.name}'s screen`}>
        <button type="button" className="office-card-expand" title="Expand their output" aria-label={`Expand ${persona.name}'s output`} onClick={() => onExpand(employee)}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
          </svg>
          Expand
        </button>
        <div className="office-card-lines">
          {screen.length > 0 ? screen.map((line, i) => <span key={i}>{line}</span>) : <span className="quiet">nothing on screen</span>}
        </div>
      </div>

      <textarea
        ref={box}
        className="office-card-prompt"
        rows={2}
        value={text}
        {...boxState(noPrompt, sending)}
        placeholder={noPrompt ? `Cannot prompt ${persona.name}: ${noPrompt}.` : `Prompt ${persona.name}… (Enter to send, Shift+Enter for a new line)`}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (!sendsOn({ key: event.key, shiftKey: event.shiftKey, ctrlKey: event.ctrlKey, isComposing: event.nativeEvent.isComposing })) return;
          event.preventDefault();
          void send();
        }}
      />
      <div className="office-card-actions">
        <button type="button" className="primary" disabled={noPrompt !== null || sending || !text.trim()} onClick={() => void send()}>
          {sending ? "Sending…" : "Send"}
        </button>
        <button type="button" disabled={!agent.pane_id} onClick={() => agent.pane_id && onGoToPane(agent.pane_id)}>
          Go to pane
        </button>
        <button type="button" title="Who they report to, what they have done, a memo" onClick={() => onDetails(employee)}>
          Details
        </button>
        <button type="button" className="danger" disabled={leaving} onClick={() => void clockOut()}>
          {leaving ? "Leaving…" : "Clock out"}
        </button>
      </div>
    </div>
  );
}
