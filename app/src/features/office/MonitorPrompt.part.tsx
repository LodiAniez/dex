import { useState, type RefObject } from "react";
import { useAgentActions } from "./agentActions";
import { promptLine } from "./monitor";
import type { Employee } from "./officeStore";

/**
 * The monitor's prompt line, drawn as the last line of its terminal. What is
 * typed stays here until Enter, then goes to the agent as the owner's turn,
 * through the same checks as the work panel's Prompt: no key reaches their pane
 * before that, so none can answer a permission dialog by accident.
 */
export function MonitorPrompt({ employee, inputRef }: { employee: Employee; inputRef: RefObject<HTMLInputElement | null> }) {
  const { prompt, sending } = useAgentActions(employee);
  const [text, setText] = useState("");
  const line = promptLine(employee.agent, sending);

  const send = async () => {
    if (!line.live || !text.trim()) return;
    if (await prompt(text)) setText("");
  };

  return (
    <div className={`monitor-prompt${line.live ? "" : " is-off"}`}>
      <span className="monitor-prompt-mark" aria-hidden>
        ›
      </span>
      {/* Read-only rather than disabled while it cannot send: a disabled input
          drops the keyboard, and Escape would then close nothing. */}
      <input
        ref={inputRef}
        className="monitor-prompt-input"
        value={text}
        readOnly={!line.live}
        aria-disabled={!line.live}
        spellCheck={false}
        autoComplete="off"
        aria-label={`Prompt for ${employee.persona.name}`}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            void send();
          }
        }}
      />
      <span className="monitor-prompt-hint">{line.hint}</span>
    </div>
  );
}
