/**
 * A working agent that has sent no hook for a while (issue #67).
 *
 * Hooks fire at every tool batch, so a gap means one tool call has been
 * running all that time: a long build, or one that will never come back - an
 * agent waiting on a command that cannot finish looks exactly like an agent
 * working hard, and did, until this line. The daemon decides when a silence
 * is long enough (`[agents] quiet_after_seconds`) and sends how long it has
 * been; here it is only put into words.
 */

/** How long a silence has lasted: `40s`, `12m`, `1h 3m`. */
export function quietSpan(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

/**
 * What to show beside a working agent's status, or null when there is nothing
 * to say - every status but `running`, and every silence the daemon judged
 * short enough to be ordinary work.
 */
export function quietWords(quietForMs: number | null | undefined): string | null {
  return typeof quietForMs === "number" ? `nothing for ${quietSpan(quietForMs)}` : null;
}
