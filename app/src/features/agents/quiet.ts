/**
 * A working agent that has sent no hook for a while (issue #67).
 *
 * Hooks fire at every tool batch, so a gap means one tool call has been
 * running all that time: a long build, or one that will never come back - an
 * agent waiting on a command that cannot finish looks exactly like an agent
 * working hard, and did, until this line.
 *
 * The daemon decides *whether* a silence is worth mentioning
 * (`[agents] quiet_after_seconds`, in `quiet_for_ms`); how long it has lasted
 * is counted here from `last_event_at`, so the number goes on rising between
 * one listing and the next instead of standing still.
 */

/** How long a silence has lasted: `40s`, `12m`, `1h 3m`. */
export function quietSpan(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

interface MaybeQuiet {
  /** The daemon's verdict: how long it had been quiet when it last looked. */
  quiet_for_ms: number | null;
  /** When Dex last heard from its hooks. */
  last_event_at: number;
}

/**
 * How long a working agent has been quiet, in words, or null when there is
 * nothing to say - every status but `running`, and every silence the daemon
 * judged short enough to be ordinary work.
 *
 * Never shorter than what the daemon said, so a clock that disagrees with the
 * daemon's cannot make the silence appear to shrink.
 */
export function quietSince(agent: MaybeQuiet, now: number): string | null {
  if (typeof agent.quiet_for_ms !== "number") return null;
  return quietSpan(Math.max(agent.quiet_for_ms, now - agent.last_event_at));
}
