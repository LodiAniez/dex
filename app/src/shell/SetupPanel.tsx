import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { DexError } from "../platform/daemon";
import { terminalChoices, useTerminal } from "../platform/runtimes";
import "./setup.css";
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
        <TerminalChoice report={report} onChosen={onRecheck} />
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

/**
 * Which terminal Dex opens, where there is a choice (WSL installed). Everything
 * new opens there: panes, and every agent spawned; panes already running keep
 * their shells until Dex next starts. A WSL distro also needs setting up for
 * agents, which the checks below then say.
 */
function TerminalChoice({ report, onChosen }: { report: DoctorReport; onChosen: () => Promise<void> }) {
  const { view, choose } = useTerminal(report);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const choices = view ? terminalChoices(view) : [];
  if (!view || choices.length === 0) return null;
  const pick = async (runtime: string) => {
    setBusy(true);
    setProblem(null);
    try {
      await choose(runtime);
      await onChosen();
    } catch (err) {
      // What happened, and the daemon's repair for it.
      setProblem(err instanceof DexError ? `${err.message}. ${err.repair}` : String(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="setup-check setup-terminal">
      <span className="setup-dot" aria-hidden />
      <span className="setup-name">terminal</span>
      <span className="setup-detail">
        Dex opens its panes, and every agent it spawns, here.
        <span className="setup-explain">
          A pane whose shell is running keeps it until Dex restarts.{problem && ` ${problem}`}
        </span>
      </span>
      <select
        className="setup-select"
        aria-label="Terminal"
        value={view.terminal}
        disabled={busy}
        onChange={(event) => void pick(event.target.value)}
      >
        {choices.map((choice) => (
          <option key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </select>
    </div>
  );
}
