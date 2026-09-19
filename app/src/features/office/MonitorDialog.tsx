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
  const dialog = useRef<HTMLDivElement>(null);

  // The keyboard goes to the prompt line, and back where it was on close - or
  // to the office, if that is gone (the agent left, and their panel with them).
  useEffect(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    input.current?.focus();
    return () => {
      if (before?.isConnected) before.focus();
      else document.querySelector<HTMLElement>(".office")?.focus();
    };
  }, []);
  // Escape closes it from anywhere on it, even after a click beside it left the
  // keyboard nowhere. Not from a dialog opened above it (the palette, settings):
  // that Escape is theirs.
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing) return;
      // Another dialog open over it (the palette, settings) takes its own Escape,
      // wherever the keyboard has got to.
      const above = [...document.querySelectorAll('[aria-modal="true"]')].some((other) => other !== dialog.current);
      const focused = document.activeElement;
      const ours = focused === null || focused === document.body || dialog.current?.contains(focused);
      if (above || !ours) return;
      event.preventDefault();
      event.stopPropagation();
      close.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  // Tab goes round the monitor's own controls, never to what is behind it.
  const trapTab = (event: React.KeyboardEvent) => {
    if (event.key !== "Tab") return;
    const stops = [...(dialog.current?.querySelectorAll<HTMLElement>("input, button") ?? [])];
    if (stops.length === 0) return;
    const at = stops.indexOf(document.activeElement as HTMLElement);
    const next = event.shiftKey ? (at <= 0 ? stops.length - 1 : at - 1) : (at + 1) % stops.length;
    event.preventDefault();
    stops[next].focus();
  };

  const status = seenAs(agent);
  return (
    // A click anywhere but its screen and controls does nothing, and leaves the keyboard where it was.
    <div
      className="monitor-scrim"
      onMouseDown={(event) => {
        if (!(event.target as HTMLElement).closest(".monitor-screen, input, button")) event.preventDefault();
      }}
    >
      <div ref={dialog} className="monitor" role="dialog" aria-modal="true" aria-label={`${persona.name}'s screen`} onKeyDown={trapTab}>
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
            <span
              className={`monitor-led is-${monitorLed(status)}`}
              role="img"
              title={STATUS_WORDS[status]}
              aria-label={STATUS_WORDS[status]}
            />
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
