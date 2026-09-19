import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useTerminal } from "../platform/runtimes";
import "./setup.css";
import { TerminalChoice } from "./TerminalChoice";
import { type Check, type DoctorReport, type Step, failures, headline, stepFor, stepLabel } from "./setup";

const STEP_EXPLAINS: Record<string, string> = {
  hooks: "Adds Dex's hooks to your Claude Code settings, so agents in Dex panes report what they are doing.",
  mcp: "Registers the Dex MCP server with Claude Code, so agents can share notes and messages.",
  skill: "Copies the dex-agentic skill into your Claude Code skills, so agents know when starting another agent helps and how to brief one.",
};

/** Why to press a step's button. */
function explains(step: Step): string {
  return step.startsWith("wsl:")
    ? `Installs the dex command, Dex's hooks, its MCP server and its skill inside ${step.slice(4)}, for agents that run there. Claude Code must be installed there first.`
    : (STEP_EXPLAINS[step] ?? "");
}

interface StepOutcome {
  ok: boolean;
  output: string;
}

export interface SetupProps {
  report: DoctorReport;
  /** Re-runs the checks; the parent owns the report. */
  onRecheck: () => Promise<void>;
  onClose: () => void;
}

/**
 * The first-run panel (PRD §14 M8): what `dex doctor` found, with a button on
 * each failure Dex can fix itself. Every button runs the same CLI command a
 * person would type, so this can never drift from the documented setup.
 */
export function Setup({ report, onRecheck, onClose }: SetupProps) {
  const [running, setRunning] = useState<Step | null>(null);
  const terminal = useTerminal(report);
  const [outcome, setOutcome] = useState<{ step: Step; result: StepOutcome } | null>(null);

  const run = async (step: Step) => {
    setRunning(step);
    setOutcome(null);
    try {
      const result = await invoke<StepOutcome>("setup_run", { step });
      setOutcome({ step, result });
      await onRecheck();
    } catch (err) {
      setOutcome({ step, result: { ok: false, output: String(err) } });
    } finally {
      setRunning(null);
    }
  };

  // Take the keyboard on open. Left in the terminal, Escape would go to xterm,
  // which keeps it, and the panel could not be closed by key at all.
  const panel = useRef<HTMLElement>(null);
  useEffect(() => panel.current?.focus(), []);

  const done = failures(report).length === 0;
  return (
    <div className="setup-backdrop" onMouseDown={onClose}>
      <section
        ref={panel}
        tabIndex={-1}
        className="setup"
        role="dialog"
        aria-label="Set up Dex"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      >
        <header className="setup-head">
          <h2>{headline(report)}</h2>
          <p className="setup-sub">
            {done
              ? "Claude Code, git, the hooks, the MCP server and the skill are all in place."
              : "Dex works with Claude Code through hooks, an MCP server and a skill. These are what a fresh install still needs."}
          </p>
        </header>
        <TerminalChoice terminal={terminal} onChosen={onRecheck} />
        <ul className="setup-checks">
          {report.checks.map((check) => (
            <CheckRow key={check.name} check={check} running={running} onRun={run} />
          ))}
        </ul>
        {outcome && (
          <pre className={`setup-output${outcome.result.ok ? "" : " is-error"}`}>{outcome.result.output || "(no output)"}</pre>
        )}
        <footer className="setup-foot">
          <button type="button" className="setup-button" onClick={() => void onRecheck()} disabled={running !== null}>
            Check again
          </button>
          <button type="button" className="setup-button primary" onClick={onClose}>
            {done ? "Done" : "Later"}
          </button>
        </footer>
      </section>
    </div>
  );
}

function CheckRow({ check, running, onRun }: { check: Check; running: Step | null; onRun: (step: Step) => void }) {
  const step = stepFor(check);
  return (
    <li className={`setup-check is-${check.status}`}>
      <span className="setup-dot" aria-label={check.status} />
      <span className="setup-name">{check.name}</span>
      <span className="setup-detail">
        {check.detail}
        {step && <span className="setup-explain">{explains(step)}</span>}
      </span>
      {step && (
        <button type="button" className="setup-button" disabled={running !== null} onClick={() => onRun(step)}>
          {running === step ? "Working…" : stepLabel(step)}
        </button>
      )}
    </li>
  );
}
