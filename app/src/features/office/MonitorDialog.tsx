import "./monitor.css";
import { useEffect, useRef } from "react";
import { STATUS_WORDS, seenAs } from "../agents";
import { monitorLed } from "./monitor";
import { MonitorPrompt } from "./MonitorPrompt.part";
import { MonitorScreen } from "./MonitorScreen.part";
import type { Employee } from "./officeStore";

/**
 * An agent's screen, opened large from their work panel: their terminal as it
 * is on their desk, on a monitor, with a line to prompt them from. A modal -
 * one at a time, and nothing behind it answers until it is closed. Their pane
 * is only watched: in the terminal view, or in a window of its own, it carries
 * on unchanged.
 */
export function MonitorDialog({ employee, onClose }: { employee: Employee; onClose: () => void }) {
  const { agent, persona, role } = employee;
  const input = useRef<HTMLInputElement>(null);

  // The keyboard goes to the prompt line, and back where it was on close.
  useEffect(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    input.current?.focus();
    return () => before?.focus();
  }, []);
  // Escape closes it wherever the keyboard is, and goes no further: a modal
  // answers before anything behind it does.
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      close.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const status = seenAs(agent);
  return (
    // A click beside the monitor does nothing, and leaves the keyboard where it was.
    <div className="monitor-scrim" onMouseDown={(event) => event.target === event.currentTarget && event.preventDefault()}>
      <div className="monitor" role="dialog" aria-modal="true" aria-label={`${persona.name}'s screen`}>
        <div className="monitor-bezel">
          <div className="monitor-glass">
            {agent.pane_id ? (
              <MonitorScreen paneId={agent.pane_id} onClick={() => input.current?.focus()} />
            ) : (
              <div className="monitor-screen monitor-nosignal">No signal: {persona.name} has no pane.</div>
            )}
            <MonitorPrompt employee={employee} inputRef={input} />
          </div>
          <div className="monitor-chin">
            <span className="monitor-brand">
              {persona.name} · {role}
            </span>
            <span className={`monitor-led is-${monitorLed(status)}`} title={STATUS_WORDS[status]} aria-label={STATUS_WORDS[status]} />
          </div>
          <button type="button" className="monitor-close" aria-label="Close" title="Close (Esc)" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="monitor-neck" aria-hidden />
        <div className="monitor-foot" aria-hidden />
      </div>
    </div>
  );
}
