/**
 * Panes a workspace needs only one of — the activity stream, the office — and
 * what "show it" therefore means.
 */

interface Panes {
  active_pane: string | null;
  panes: readonly { id: string; kind: string }[];
}

/**
 * Focus the pane already showing `kind`, or say which pane to split one off:
 * the focused pane, else the first. Null when the workspace has no panes.
 */
export function singletonTarget(workspace: Panes, kind: string): { focus: string } | { split: string } | null {
  const existing = workspace.panes.find((pane) => pane.kind === kind);
  if (existing) return { focus: existing.id };
  const from = workspace.active_pane ?? workspace.panes[0]?.id;
  return from ? { split: from } : null;
}
