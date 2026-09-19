import { useEffect, useRef, useState } from "react";
import { agentInPane, useAgents } from "../features/agents";
import { DexError } from "../platform/daemon";
import type { TerminalView } from "../platform/generated/TerminalView";
import {
  type RestartChoice,
  restartChoices,
  runtimeLabel,
  stillToRestart,
  terminalChoices,
  type useTerminal,
} from "../platform/runtimes";
import { canRestartIn, restartIn } from "../platform/terminalRegistry";
import "./setup.css";

/** The chosen terminal and a way to choose, as `useTerminal` gives them. */
export type Terminal = ReturnType<typeof useTerminal>;

/**
 * Which terminal Dex opens, where there is a choice (WSL installed). Everything
 * new opens there: panes, and every agent spawned. Panes already running keep
 * their shell, and are offered for a restart: plain shells ticked, busy ones
 * not (`workspace/switching.rs` in dex-core). A WSL distro also needs setting up for
 * agents, which the checks below then say.
 */
export function TerminalChoice({ terminal, onChosen }: { terminal: Terminal; onChosen: () => Promise<void> }) {
  const { view, choose } = terminal;
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
