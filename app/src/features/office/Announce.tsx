import { useEffect, useRef, useState } from "react";
import { request } from "../../platform/daemon";
import type { Employee } from "./officeStore";
import { announcePlan, announceSummary, midQuestion, promptArgs } from "./prompt";

/** A loudspeaker horn: the office's public address. */
export function Horn({ size }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 48 48" aria-hidden="true">
      <path d="M8 19 h8 l18 -10 v30 l-18 -10 h-8 a3 3 0 0 1 -3 -3 v-4 a3 3 0 0 1 3 -3 z" fill="#f0bf74" />
      <path d="M16 19 v10" stroke="#b07a48" strokeWidth="2" />
      <rect x="10" y="29" width="7" height="11" rx="3" fill="#b07a48" />
      <path d="M38 17 q6 7 0 14" stroke="#f0bf74" strokeWidth="3" strokeLinecap="round" fill="none" />
      <path d="M42 12 q10 12 0 24" stroke="#f0bf74" strokeWidth="3" strokeLinecap="round" fill="none" opacity="0.6" />
    </svg>
  );
}

/**
 * One prompt to everyone: typed into each agent's terminal as the owner's
 * turn. Whoever is waiting on a dialog is left out — typed text would answer
 * the dialog — and named, so the owner knows who did not hear it.
 */
export function AnnounceDialog({ staff, onClose }: { staff: readonly Employee[]; onClose: () => void }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => field.current?.focus(), []);

  const plan = announcePlan(staff);
  const asking = midQuestion(plan.to);
  const ready = plan.to.length > 0 && text.trim() !== "" && !busy;
  const announce = async () => {
    if (!ready) return;
    setBusy(true);
    const failed: string[] = [];
    for (const member of plan.to) {
      const args = promptArgs(member.agent.id, text);
      if (!args) continue;
      try {
        // The daemon checks again at the moment of typing: someone may have
        // reached a dialog, or gone, since this list was drawn.
        await request("agent.prompt", args);
      } catch {
        failed.push(member.persona.name);
      }
    }
    setBusy(false);
    setResult(announceSummary(plan) + (failed.length > 0 ? ` Could not reach ${failed.join(", ")}.` : ""));
  };

  return (
    <div className="office-scrim" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="office-hire"
        aria-label="Announce to everyone"
        onSubmit={(event) => {
          event.preventDefault();
          void announce();
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) void announce();
        }}
      >
        <div className="office-hire-title">
          <Horn size={22} /> Announce to everyone
        </div>
        {result === null ? (
          <>
            <label>
              A prompt for every agent, typed into each terminal as your turn.
              <textarea ref={field} rows={4} value={text} placeholder="Stop and commit what you have; we are changing approach." onChange={(event) => setText(event.target.value)} />
            </label>
            <div className="office-announce-who">{announceSummary(plan).replace(/^Announced to/, "Goes to")}</div>
            {asking.length > 0 && (
              <div className="office-announce-asking">
                {asking.map((member) => member.persona.name).join(", ")} {asking.length === 1 ? "has" : "have"} asked you something and may take this for the answer.
              </div>
            )}
          </>
        ) : (
          <div className="office-announce-result">{result}</div>
        )}
        <div className="office-hire-actions">
          <button type="button" onClick={onClose}>
            {result === null ? "Cancel" : "Close"}
          </button>
          {result === null && (
            <button type="submit" className="primary" disabled={!ready}>
              {busy ? "Announcing…" : "Announce"}
            </button>
          )}
        </div>
      </form>
    </div>
  );
}
