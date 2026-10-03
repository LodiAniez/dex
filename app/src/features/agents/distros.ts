/**
 * What to say when a distro stops answering (issue #73).
 *
 * The office goes still when a distro wedges - no status changes, no agents
 * ending - and the owner reading a still office concluded Dex itself had
 * hung, closed it, and lost an hour to looking in the wrong place. Dex knows
 * which distro went quiet; this turns that into one line.
 *
 * Only what is *newly* quiet, because the daemon reports the same list every
 * sweep while the silence lasts, and a warning raised four times a minute is
 * noise rather than news.
 */
export function newlyQuiet(before: string[] | undefined, after: string[] | undefined): string | null {
  const was = new Set(before ?? []);
  const now = (after ?? []).filter((distro) => !was.has(distro));
  if (now.length === 0) return null;
  const named = now.join(", ");
  const is = now.length === 1 ? "is" : "are";
  return `${named} ${is} not answering. Agents in there may still be working; Dex cannot tell until it answers again.`;
}
