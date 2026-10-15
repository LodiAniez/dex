import { describe, expect, it } from "vitest";
import type { PaneView } from "../platform/generated/PaneView";
import type { WorkspaceList } from "../platform/generated/WorkspaceList";
import type { WorkspaceView } from "../platform/generated/WorkspaceView";
import { shellPanes } from "./shellPanes";

const pane = (id: string, kind = "terminal"): PaneView => ({ id, label: null, cwd: `C:/work/${id}`, kind, runtime: "windows" });

const workspace = (id: string, panes: PaneView[]): WorkspaceView => ({
  id,
  name: id,
  color: null,
  root_path: "C:/work",
  sort_index: 0,
  layout: null,
  active_pane: panes[0]?.id ?? null,
  panes,
});

const list = (active: string | null, ...workspaces: WorkspaceView[]): WorkspaceList => ({ workspaces, active, revision: 1 });

const ids = (panes: { paneId: string }[]) => panes.map((it) => it.paneId);

describe("shellPanes", () => {
  it("names every terminal pane, in the workspaces nobody is looking at as well as the one on screen", () => {
    // Issue #92: only the active workspace's layout mounts, so a pane
    // elsewhere had no shell - it took no input, and an agent spawned into it
    // was never launched at all.
    const panes = shellPanes(list("on-screen", workspace("on-screen", [pane("a")]), workspace("away", [pane("b"), pane("c")])), new Set());
    expect(ids(panes)).toEqual(["a", "b", "c"]);
  });

  it("starts with the workspace on screen, whose panes somebody is waiting to see", () => {
    const panes = shellPanes(list("second", workspace("first", [pane("a")]), workspace("second", [pane("b")])), new Set());
    expect(ids(panes)).toEqual(["b", "a"]);
  });

  it("leaves out the panes that run no shell", () => {
    const kinds = [pane("term"), pane("log", "activity"), pane("changes", "diff"), pane("notes", "markdown"), pane("later", "something-new")];
    const panes = shellPanes(list("one", workspace("one", kinds)), new Set());
    expect(ids(panes)).toEqual(["term"]);
  });

  it("leaves a pane in a window of its own to that window", () => {
    const panes = shellPanes(list("one", workspace("one", [pane("here"), pane("out")])), new Set(["out"]));
    expect(ids(panes)).toEqual(["here"]);
  });

  it("names nothing before the workspaces have arrived", () => {
    expect(shellPanes(null, new Set())).toEqual([]);
  });

  it("carries what starting a shell needs: where it runs, and in which workspace", () => {
    const panes = shellPanes(list("one", workspace("one", [pane("a")])), new Set());
    expect(panes).toEqual([{ paneId: "a", workspaceId: "one", cwd: "C:/work/a", runtime: "windows" }]);
  });
});
