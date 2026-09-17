import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentList } from "../../platform/generated/AgentList";
import type { AgentStatus } from "../../platform/generated/AgentStatus";
import type { AgentView } from "../../platform/generated/AgentView";

// Hoisted above the imports by vitest, so `notifications` sees the fake.
const invoke = vi.hoisted(() =>
  vi.fn((_command: string, _args: Record<string, unknown>) => Promise.resolve()),
);
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { notifyTransitions, type PaneContext } from "./notifications";

function agent(id: string, status: AgentStatus, extra: Partial<AgentView> = {}): AgentView {
  return {
    id,
    pane_id: `pane-${id}`,
    workspace_id: "ws",
    label: id,
    backend: "claude",
    status,
    status_detail: null,
    status_at: 0,
    permission_mode: "auto",
    task_brief: null,
    started: true,
    parent_id: null,
    depth: 0,
    started_at: 0,
    ended_at: null,
    ...extra,
  };
}

function list(...agents: AgentView[]): AgentList {
  return { agents, revision: 1 };
}

/** Every pane exists, and the user is looking at none of them. */
const elsewhere = (paneId: string): PaneContext => ({
  workspaceId: "ws",
  workspaceName: "api",
  paneName: paneId,
  looking: false,
});

function bodies(): string[] {
  return invoke.mock.calls.map((call) => (call[1] as { body: string }).body);
}

beforeEach(() => invoke.mockClear());

describe("toasts for agents needing attention", () => {
  it("says nothing on the first load", () => {
    // Those are current states, not things that just happened — otherwise
    // starting Dex would toast once for every agent already sitting idle.
    notifyTransitions(null, list(agent("a", "waiting")), elsewhere);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("toasts an agent that has just started waiting", () => {
    notifyTransitions(list(agent("a", "running")), list(agent("a", "waiting")), elsewhere);
    expect(bodies()).toEqual(["Claude needs your input"]);
  });

  it("toasts an agent that has just finished", () => {
    notifyTransitions(list(agent("a", "running")), list(agent("a", "idle")), elsewhere);
    expect(bodies()).toEqual(["Claude finished"]);
  });

  it("includes why, when there is a why", () => {
    const failed = agent("a", "error", { status_detail: "rate_limit" });
    notifyTransitions(list(agent("a", "running")), list(failed), elsewhere);
    expect(bodies()).toEqual(["Claude stopped with an error: rate_limit"]);
  });

  it("says nothing for a status that is not worth interrupting for", () => {
    notifyTransitions(list(agent("a", "idle")), list(agent("a", "running")), elsewhere);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("says nothing when the status has not changed", () => {
    // The list is re-read on every change to any agent, so this is the common
    // case, not an edge one.
    notifyTransitions(list(agent("a", "waiting")), list(agent("a", "waiting")), elsewhere);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("says nothing for an agent that has only just appeared", () => {
    // A new agent's row starts idle, which is not "finished".
    notifyTransitions(list(agent("a", "running")), list(agent("a", "running"), agent("b", "idle")), elsewhere);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("says nothing about a pane the user is looking at", () => {
    const watching = (paneId: string): PaneContext => ({ ...elsewhere(paneId), looking: true });
    notifyTransitions(list(agent("a", "running")), list(agent("a", "waiting")), watching);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("says nothing about a pane that is gone", () => {
    notifyTransitions(list(agent("a", "running")), list(agent("a", "waiting")), () => null);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("says nothing about an agent with no pane", () => {
    const orphan = agent("a", "waiting", { pane_id: null });
    notifyTransitions(list(agent("a", "running")), list(orphan), elsewhere);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("calls the agent by the name the office knows it by, when it is given one", () => {
    const names = (id: string) => (id === "a" ? "Pip" : undefined);
    const before = list(agent("a", "running"), agent("b", "running"));
    notifyTransitions(before, list(agent("a", "waiting"), agent("b", "idle")), elsewhere, names);
    // The title still says which pane; someone who has never opened the office
    // is told "Claude", as before.
    expect(bodies()).toEqual(["Pip needs your input", "Claude finished"]);
  });

  it("toasts each agent that changed, and only those", () => {
    const before = list(agent("a", "running"), agent("b", "running"), agent("c", "waiting"));
    const after = list(agent("a", "waiting"), agent("b", "running"), agent("c", "waiting"));
    notifyTransitions(before, after, elsewhere);
    expect(bodies()).toEqual(["Claude needs your input"]);
  });

  it("names the workspace and pane, and points the click at both", () => {
    notifyTransitions(list(agent("a", "running")), list(agent("a", "waiting")), elsewhere);
    expect(invoke).toHaveBeenCalledWith("notify_agent", {
      title: "api · pane-a",
      body: "Claude needs your input",
      workspace: "ws",
      pane: "pane-a",
    });
  });

  it("says an agent that ended its turn on a question asked something, not that it finished", () => {
    const asked = agent("a", "idle", { status_detail: "asked you: Should I run it against staging?" });
    notifyTransitions(list(agent("a", "running")), list(asked), elsewhere);
    expect(bodies()).toEqual(["Claude asked you: Should I run it against staging?"]);
  });
});
