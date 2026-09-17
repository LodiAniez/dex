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

/**
 * Arguments for `agent.spawn`, or null when there is no task. The new pane is
 * split off a terminal — the focused one if it is one — and never off the
 * office, which is what usually has focus when this is pressed.
 */
export function hireArgs(workspace: Workspace, task: string, label: string): HireArgs | null {
  const brief = task.trim();
  if (!brief) return null;
  const terminals = workspace.panes.filter((pane) => pane.kind === "terminal");
  const from = terminals.find((pane) => pane.id === workspace.active_pane) ?? terminals[0];
  const args: HireArgs = { task: brief, workspace: workspace.id };
  if (from) args.pane = from.id;
  if (label.trim()) args.label = label.trim();
  return args;
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
