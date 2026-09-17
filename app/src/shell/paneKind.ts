/**
 * What a pane's kind means to the shell around it: what its header says, and
 * whether it runs a shell at all.
 */

/** The last two segments of a path: enough to tell panes apart. */
export function shortPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts.length <= 2 ? path : `…/${parts.slice(-2).join("/")}`;
}

/** What the header says a pane is: its kind for the special ones, else where it is. */
export function paneTitle(pane: { kind: string; cwd: string }): string {
  switch (pane.kind) {
    case "activity":
      return "activity";
    case "office":
      return "office";
    case "diff":
      return `diff · ${shortPath(pane.cwd)}`;
    case "markdown":
      return pane.cwd.split("/").pop() ?? pane.cwd;
    default:
      return shortPath(pane.cwd);
  }
}

/**
 * Whether a pane of this kind mounts a terminal. Only `terminal` does: a kind
 * this build does not know gets an explanation, never a shell nobody asked for.
 */
export function runsShell(kind: string): boolean {
  return kind === "terminal";
}
