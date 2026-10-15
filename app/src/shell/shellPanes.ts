/**
 * Which panes this window starts a shell for, and in what order.
 *
 * A pane's shell used to start when its terminal first attached, and only the
 * workspace on screen is ever mounted - so a pane in any other workspace had
 * nothing running in it: `pane.send` refused it, and an agent spawned there
 * was never launched at all and died "never started" half a minute later
 * (issue #92). Every terminal pane gets its shell, wherever it is. The
 * workspace on screen comes first, because its panes are the ones somebody is
 * waiting to see.
 *
 * Panes in windows of their own are left out: that window holds the terminal
 * (`popouts.ts`), and this one must not start a second shell underneath it.
 */
import type { WorkspaceList } from "../platform/generated/WorkspaceList";
import { runsShell } from "./paneKind";

/** A pane whose shell this window starts, and what starting it needs. */
export interface ShellPane {
  paneId: string;
  workspaceId: string;
  cwd: string;
  /** Where its shell runs: `windows` or `wsl:<distro>`. */
  runtime: string;
}

/** Every pane that should have a shell right now, the visible workspace's first. */
export function shellPanes(list: WorkspaceList | null, detached: ReadonlySet<string>): ShellPane[] {
  if (!list) return [];
  // Stable, so the workspaces keep their sidebar order behind the active one.
  const order = [...list.workspaces].sort((a, b) => Number(b.id === list.active) - Number(a.id === list.active));
  return order.flatMap((workspace) =>
    workspace.panes
      .filter((pane) => runsShell(pane.kind) && !detached.has(pane.id))
      .map((pane) => ({ paneId: pane.id, workspaceId: workspace.id, cwd: pane.cwd, runtime: pane.runtime })),
  );
}
