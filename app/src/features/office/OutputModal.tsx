import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { readTail } from "../../platform/terminalRegistry";
import { AgentStatusDot, STATUS_WORDS } from "../agents";
import { useAgentActions } from "./agentActions";
import { Avatar } from "./Avatar";
import type { Employee } from "./officeStore";
import { sendsOn, whyNoPrompt } from "./prompt";
import { followsBottom, screenText } from "./screen";

/** How far back the expanded view reaches into the terminal's history. */
const TAIL_LINES = 500;
const GLANCE_MS = 1000;

/** The pane's recent output, scrollback included, re-read once a second. */
function useTail(paneId: string | null): string[] {
  const [lines, setLines] = useState<string[]>([]);
  useEffect(() => {
    if (!paneId) return;
    const glance = () => {
      const next = screenText(readTail(paneId, TAIL_LINES), TAIL_LINES);
      setLines((current) => (current.join("\n") === next.join("\n") ? current : next));
    };
    glance();
    const timer = setInterval(glance, GLANCE_MS);
    return () => clearInterval(timer);
  }, [paneId]);
  return lines;
}

interface Props {
  employee: Employee;
  onGoToPane: (paneId: string) => void;
  onClose: () => void;
}

/**
 * An agent's output, expanded: what their card shows, with room - and with the
 * history behind the visible screen, which a card has no space for. Live, and
 * following the newest line unless the owner has scrolled up to read. The
 * prompt box comes along, so reading and replying happen in one place.
 */
export function OutputModal({ employee, onGoToPane, onClose }: Props) {
  const { agent, persona, role } = employee;
  const lines = useTail(agent.pane_id);
  const { prompt, sending } = useAgentActions(employee);
  const [text, setText] = useState("");
  const noPrompt = whyNoPrompt(agent);

  // Follows the bottom while the reader is there; leaves them alone once they scroll up.
  const scroller = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  useLayoutEffect(() => {
    const view = scroller.current;
    if (view && following.current) view.scrollTop = view.scrollHeight;
  }, [lines]);

  // Takes focus when it opens: a terminal that kept it would eat Escape.
  const box = useRef<HTMLDivElement>(null);
  // Without scrolling anything to do it: the office underneath must stay put.
  useEffect(() => box.current?.focus({ preventScroll: true }), []);

  const send = async () => {
    if (await prompt(text)) {
      setText("");
      following.current = true;
    }
  };

  return (
    <div className="office-scrim" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div
        ref={box}
        className="office-output"
        role="dialog"
        aria-label={`${persona.name}'s output`}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      >
        <div className="office-output-head">
          <Avatar persona={persona} size={30} />
          <span className="office-card-who">
            <span className="office-card-name">{persona.name}</span>
            <span className="office-card-role">{role}</span>
          </span>
          <span className="office-card-status">
            <AgentStatusDot status={agent.status} />
            {STATUS_WORDS[agent.status]}
          </span>
          <button type="button" disabled={!agent.pane_id} onClick={() => agent.pane_id && onGoToPane(agent.pane_id)}>
            Go to pane
          </button>
          <button type="button" className="office-panel-close" aria-label="Close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div
          ref={scroller}
          className="office-output-lines"
          onScroll={(event) => {
            following.current = followsBottom(event.currentTarget);
          }}
        >
          {lines.length > 0 ? lines.map((line, i) => <div key={i}>{line === "" ? " " : line}</div>) : <div className="quiet">nothing on screen</div>}
        </div>
        <div className="office-output-prompt">
          <textarea
            className="office-card-prompt"
            rows={2}
            value={text}
            disabled={noPrompt !== null || sending}
            placeholder={noPrompt ? `Cannot prompt ${persona.name}: ${noPrompt}.` : `Prompt ${persona.name}… (Enter to send, Shift+Enter for a new line)`}
            onChange={(event) => setText(event.target.value)}
            onKeyDown={(event) => {
              // Escape in the box closes the modal too; it is not a second layer.
              if (!sendsOn({ key: event.key, shiftKey: event.shiftKey, ctrlKey: event.ctrlKey, isComposing: event.nativeEvent.isComposing })) return;
              event.preventDefault();
              void send();
            }}
          />
          <button type="button" className="primary" disabled={noPrompt !== null || sending || !text.trim()} onClick={() => void send()}>
            {sending ? "Sending…" : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}
