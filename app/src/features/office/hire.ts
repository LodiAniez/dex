/**
 * What the office's two buttons send: a human hiring an agent through HR, and
 * a human writing one a memo. Both go down the same daemon paths the CLI uses
 * (`agent.spawn`, `context.message_send`), so the office adds no way of doing
 * anything that was not already there.
 */

interface Workspace {
  id: string;
  active_pane: string | null;
  panes: readonly { id: string; kind: string }[];
}

export interface HireArgs {
  task: string;
  workspace: string;
  /** The terminal the new pane is split off. */
  pane?: string;
  label?: string;
}

/** The terminal a hire is split off: the focused one, else the first. */
function hireSource(workspace: Workspace) {
  const terminals = workspace.panes.filter((pane) => pane.kind === "terminal");
  return terminals.find((pane) => pane.id === workspace.active_pane) ?? terminals[0];
}


/**
 * Arguments for `agent.spawn`, or null when there is no task. The new pane is
 * split off a terminal — the focused one if it is one — and never off the
 * office, which is what usually has focus when this is pressed.
 */
export function hireArgs(workspace: Workspace, task: string, label: string): HireArgs | null {
  const brief = task.trim();
  if (!brief) return null;
  const from = hireSource(workspace);
  const args: HireArgs = { task: brief, workspace: workspace.id };
  if (from) args.pane = from.id;
  if (label.trim()) args.label = label.trim();
  return args;
}

/**
 * Why a pane label cannot be used, and one that can, or null when it can.
 * Labels belong to panes, and the office shows agents: a pane left behind by an
 * agent that has ended still holds its label, where nobody in the office can see
 * it. "Already used", with no agent in sight, reads as a bug - so the form says
 * what is holding the label before the daemon has to refuse it.
 */
/** The daemon's limit on a pane label (`clean_label`): one word, this long at most. */
const MAX_LABEL_CHARS = 32;

export function labelClash(
  panes: readonly { id: string; label: string | null }[],
  wanted: string,
  panesWithAgents: ReadonlySet<string>,
): { why: string; free: string } | null {
  const label = wanted.trim();
  if (!label) return null;
  const taken = new Set(panes.map((pane) => pane.label));
  // The label itself if nobody holds it, else the first numbered one that is free - always within the limit.
  const free = (base: string): string => {
    if (!taken.has(base)) return base;
    for (let next = 2; ; next += 1) {
      const suffix = `-${next}`;
      const candidate = `${base.slice(0, MAX_LABEL_CHARS - suffix.length)}${suffix}`;
      if (!taken.has(candidate)) return candidate;
    }
  };
  // The daemon's other two rules, said here for the same reason: its refusal comes after the button.
  if (/\s/.test(label)) {
    return { why: "A label is one word: other agents type it to reach this one.", free: free(label.replace(/\s+/g, "-").slice(0, MAX_LABEL_CHARS)) };
  }
  if ([...label].length > MAX_LABEL_CHARS) {
    return { why: `A label is at most ${MAX_LABEL_CHARS} characters.`, free: free([...label].slice(0, MAX_LABEL_CHARS).join("")) };
  }
  const holder = panes.find((pane) => pane.label === label);
  if (!holder) return null;
  const why = panesWithAgents.has(holder.id)
    ? `An agent's pane is already labelled "${label}".`
    : `A pane labelled "${label}" is still open in Terminal view, with no agent in it. Labels belong to panes.`;
  return { why, free: free(label) };
}

/** Whether a failed hire failed because every seat is taken. */
export function isHiringFreeze(error: unknown): boolean {
  return typeof error === "object" && error !== null && "code" in error && error.code === "concurrency_limit";
}

/** Arguments for `context.message_send` from the human, or null when nothing was written. */
export function memoArgs(
  workspaceId: string,
  agentId: string,
  body: string,
): { target_agent: string; body: string; workspace: string } | null {
  const text = body.trim();
  return text ? { target_agent: agentId, body: text, workspace: workspaceId } : null;
}

/**
 * Arguments for `agent.stop` as the office asks it: Claude Code is told to
 * leave (`/exit`) so its session ends properly, and the pane goes with it -
 * which is the owner's own, confirmed, choice, so it holds for a pane they
 * opened themselves as well.
 */
export function clockOutArgs(agentId: string): { agent: string; graceful: true; close_pane: true } {
  return { agent: agentId, graceful: true, close_pane: true };
}

/** What is asked before clocking anyone out. It cannot be undone, so it says what will happen. */
export function clockOutQuestion(name: string, role: string): string {
  return `Clock out ${name} (${role})? Dex sends /exit to end their Claude Code session and closes their pane. Their notes and any branch they worked on stay.`;
}
