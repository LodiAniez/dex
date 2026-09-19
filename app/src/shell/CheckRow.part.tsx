import { type Check, type Step, stepFor, stepLabel } from "./setup";

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

/** One of `dex doctor`'s checks, with the button that fixes it where Dex can. */
export function CheckRow({ check, running, onRun }: { check: Check; running: Step | null; onRun: (step: Step) => void }) {
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
