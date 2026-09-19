import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { DexError } from "../platform/daemon";
import { agentInPane, useAgents } from "../features/agents";
import type { TerminalView } from "../platform/generated/TerminalView";
import {
  type RestartChoice,
  restartChoices,
  runtimeLabel,
  stillToRestart,
  terminalChoices,
  useTerminal,
} from "../platform/runtimes";
import { canRestartIn, restartIn } from "../platform/terminalRegistry";
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
 * new opens there: panes, and every agent spawned. Panes already running keep
 * their shell, and are offered for a restart: plain shells ticked, busy ones
 * not (`workspace/switching.rs` in dex-core). A WSL distro also needs setting up for
 * agents, which the checks below then say.
 */
function TerminalChoice({ report, onChosen }: { report: DoctorReport; onChosen: () => Promise<void> }) {
  const { view, choose } = useTerminal(report);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  // After a choice: running panes to restart in it, or not, as the owner ticks.
  const [offer, setOffer] = useState<{ runtime: string; choices: RestartChoice[]; others: number } | null>(null);
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const agents = useAgents();
  const offerRef = useRef<HTMLDivElement>(null);
  // The question is the next thing to answer: take the keyboard to it.
  useEffect(() => {
    if (offer) offerRef.current?.focus();
  }, [offer]);
  const choices = view ? terminalChoices(view) : [];
  if (!view || choices.length === 0) return null;
  const agentIn = (pane: string) => {
    const agent = agentInPane(agents, pane);
    return agent ? (agent.label ?? "an agent") : null;
  };
  const offered = (running: TerminalView["running_elsewhere"], runtime: string) =>
    restartChoices(running, (pane) => canRestartIn(pane, runtime), agentIn);
  const pick = async (runtime: string) => {
    setBusy(true);
    setProblem(null);
    setOffer(null);
    try {
      const answer = await choose(runtime);
      const restartable = offered(answer.running_elsewhere, runtime);
      // Panes elsewhere this window cannot restart: shown in another window.
      const others = answer.running_elsewhere.length - restartable.length;
      if (restartable.length > 0 || others > 0) {
        setOffer({ runtime, choices: restartable, others });
        setTicked(new Set(restartable.filter((choice) => choice.ticked).map((choice) => choice.pane)));
      }
      await onChosen();
    } catch (err) {
      // What happened, and the daemon's repair for it.
      setProblem(err instanceof DexError ? `${err.message}. ${err.repair}` : String(err));
    } finally {
      setBusy(false);
    }
  };
  // Asked again at the click: the list may be minutes old.
  const restart = async () => {
    if (!offer) return;
    setBusy(true);
    try {
      const now = await choose(offer.runtime);
      const { restart: panes, skipped } = stillToRestart(ticked, offer.choices, offered(now.running_elsewhere, offer.runtime));
      for (const pane of panes) restartIn(pane, offer.runtime);
      setProblem(skipped.length > 0 ? `Left as they were - something started in them since: ${skipped.join(", ")}.` : null);
      setOffer(null);
    } catch (err) {
      setProblem(err instanceof DexError ? `${err.message}. ${err.repair}` : String(err));
    } finally {
      setBusy(false);
    }
  };
  const toggle = (pane: string) =>
    setTicked((now) => {
      const next = new Set(now);
      if (next.has(pane)) next.delete(pane);
      else next.add(pane);
      return next;
    });
  return (
    <div className="setup-terminal-block">
      <div className="setup-check setup-terminal">
        <span className="setup-dot" aria-hidden />
        <span className="setup-name">terminal</span>
        <span className="setup-detail">
          Dex opens its panes, and every agent it spawns, here.
          <span className="setup-explain">
            Panes already running keep their shell unless you restart them.{problem && ` ${problem}`}
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
      {offer && (
        <div
          className="setup-restart"
          role="group"
          tabIndex={-1}
          ref={offerRef}
          aria-label={`Restart panes in ${runtimeLabel(offer.runtime)}`}
        >
          <p className="setup-restart-head">
            Restart these in {runtimeLabel(offer.runtime)}? Each starts again in its folder, with its scrollback kept;
            whatever runs in it ends.
          </p>
          <ul>
            {offer.choices.map((choice) => (
              <li key={choice.pane}>
                <label>
                  <input type="checkbox" checked={ticked.has(choice.pane)} onChange={() => toggle(choice.pane)} />
                  <span className="setup-restart-title">{choice.title}</span>
                  <span className={`setup-restart-detail${choice.busy ? " is-busy" : ""}`}>{choice.detail}</span>
                </label>
              </li>
            ))}
          </ul>
          {offer.others > 0 && (
            <p className="setup-restart-head">
              {offer.others === 1 ? "1 pane in another window keeps its" : `${offer.others} panes in other windows keep their`}{" "}
              shell until Dex restarts.
            </p>
          )}
          <div className="setup-restart-actions">
            <button type="button" className="setup-button" onClick={() => setOffer(null)}>
              Not now
            </button>
            <button
              type="button"
              className="setup-button primary"
              disabled={ticked.size === 0 || busy}
              onClick={() => void restart()}
            >
              Restart {ticked.size === 1 ? "1 pane" : `${ticked.size} panes`}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
