import { useEffect, useState } from "react";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import type { AgentView } from "../../platform/generated/AgentView";
import { seenAs } from "./attention";

/** What each status means to the person looking at it. */
export const STATUS_WORDS: Record<AgentStatus, string> = {
  idle: "idle",
  running: "working",
  waiting: "needs you",
  error: "stopped",
  unknown: "not responding",
  dead: "ended",
};

/** The status dot — the most important pixel in the app (PRD §13), so it is bright and pulses when it needs you. */
export function AgentStatusDot({ status }: { status: AgentStatus }) {
  return <span className={`agent-dot ${status}`} aria-label={STATUS_WORDS[status]} />;
}

/** Dot, state, time in state, and permission mode, for a pane header. */
export function AgentBadge({ agent }: { agent: AgentView }) {
  const now = useNow(5000);
  // As the owner should see it: an agent that asked them something is waiting for them.
  const shown = seenAs(agent);
  const detail = agent.status === "error" && agent.status_detail ? ` (${agent.status_detail})` : "";
  const mode = agent.permission_mode && agent.permission_mode !== "default" ? agent.permission_mode : null;
  return (
    <span className={`agent-badge ${shown}`} title={`${agent.backend}: ${STATUS_WORDS[shown]}${detail}`}>
      <AgentStatusDot status={shown} />
      <span className="agent-state">
        {STATUS_WORDS[shown]}
        {detail}
      </span>
      <span className="agent-elapsed">{elapsed(now - agent.status_at)}</span>
      {mode && <span className="agent-mode">{mode}</span>}
    </span>
  );
}

/** The current time, refreshed every `ms`. */
function useNow(ms: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(timer);
  }, [ms]);
  return now;
}

function elapsed(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}
