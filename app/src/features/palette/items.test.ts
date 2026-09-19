import { describe, expect, it } from "vitest";
import type { WorkspaceList } from "../../platform/generated/WorkspaceList";
import { ACTIONS, SHIPPED_KEYMAP } from "../../shell/keybindings";
import { COMMAND_LABELS, buildItems, parseQuery, rankItems } from "./items";

const list: WorkspaceList = {
  revision: 1,
  active: "ws-api",
  workspaces: [
    {
      id: "ws-api",
      name: "api",
      color: null,
      root_path: "C:/src/api",
      sort_index: 0,
      layout: null,
      active_pane: "pane-1",
      panes: [
        { id: "pane-1", label: "lead", cwd: "C:/src/api", kind: "terminal", runtime: "windows" },
        { id: "pane-2", label: null, cwd: "C:/src/api/worker", kind: "terminal", runtime: "windows" },
        { id: "pane-3", label: null, cwd: "C:/src/api", kind: "activity", runtime: "windows" },
      ],
    },
    {
      id: "ws-web",
      name: "web",
      color: null,
      root_path: "C:/src/web",
      sort_index: 1,
      layout: null,
      active_pane: null,
      panes: [{ id: "pane-4", label: "frontend", cwd: "C:/src/web", kind: "terminal", runtime: "windows" }],
    },
  ],
};

const items = buildItems(list, SHIPPED_KEYMAP);

describe("reading the query", () => {
  it("has no scope by default", () => {
    expect(parseQuery("split")).toEqual({ scope: "all", needle: "split" });
  });

  it("scopes to workspaces with w: and panes with p:", () => {
    expect(parseQuery("w:api")).toEqual({ scope: "workspace", needle: "api" });
    expect(parseQuery("p: lead")).toEqual({ scope: "pane", needle: "lead" });
    expect(parseQuery("W:api")).toEqual({ scope: "workspace", needle: "api" });
  });

  it("treats a bare prefix as that scope with nothing typed yet", () => {
    expect(parseQuery("w:")).toEqual({ scope: "workspace", needle: "" });
  });

  it("does not mistake a colon elsewhere for a scope", () => {
    expect(parseQuery("a:b")).toEqual({ scope: "all", needle: "a:b" });
  });
});

describe("what the palette offers", () => {
  it("lists workspaces, then panes, then commands", () => {
    const kinds = items.map((item) => item.kind);
    const firstPane = kinds.indexOf("pane");
    const firstCommand = kinds.indexOf("command");
    expect(kinds.slice(0, firstPane).every((kind) => kind === "workspace")).toBe(true);
    expect(kinds.slice(firstPane, firstCommand).every((kind) => kind === "pane")).toBe(true);
    expect(kinds.slice(firstCommand).every((kind) => kind === "command")).toBe(true);
  });

  it("names an unlabelled pane by its directory", () => {
    const pane = items.find((item) => item.kind === "pane" && item.id === "pane-2");
    expect(pane?.title).toBe("worker");
  });

  it("marks what is current", () => {
    const current = items.filter((item) => "current" in item && item.current);
    expect(current.map((item) => ("id" in item ? item.id : ""))).toEqual(["ws-api", "pane-1"]);
  });

  it("shows each command's binding, as bound right now", () => {
    const close = items.find((item) => item.kind === "command" && item.action === "close-pane");
    expect(close?.detail).toBe("Ctrl+Shift+W");
  });

  it("does not list itself", () => {
    expect(items.some((item) => item.kind === "command" && item.action === "command-palette")).toBe(false);
  });

  it("has a name for every command", () => {
    // A command without a label would not be findable, and nobody would notice.
    for (const action of Object.keys(ACTIONS)) {
      expect(COMMAND_LABELS[action], `${action} has no label`).toBeTruthy();
    }
  });

  it("offers the activity popup", () => {
    expect(COMMAND_LABELS["open-activity"]).toBe("Show activity");
  });

  it("offers each view by the name people will type", () => {
    expect(COMMAND_LABELS["view-terminal"]).toBe("Terminal view");
    expect("view-cards" in COMMAND_LABELS).toBe(false);
    expect(COMMAND_LABELS["view-office"]).toBe("Office view");
    expect(COMMAND_LABELS["cycle-view"]).toBe("Next view");
  });

  it("offers commands even before the workspaces have loaded", () => {
    const early = buildItems(null, SHIPPED_KEYMAP);
    expect(early.length).toBeGreaterThan(0);
    expect(early.every((item) => item.kind === "command")).toBe(true);
  });
});

describe("ranking", () => {
  it("returns everything in order for an empty query", () => {
    expect(rankItems(items, "").map((r) => r.item)).toEqual(items);
  });

  it("finds a workspace by name", () => {
    const [first] = rankItems(items, "web");
    expect(first.item).toMatchObject({ kind: "workspace", id: "ws-web" });
  });

  it("finds a command by its initials", () => {
    const [first] = rankItems(items, "spr");
    expect(first.item).toMatchObject({ kind: "command", action: "split-right" });
  });

  it("honours the scope", () => {
    const panes = rankItems(items, "p:");
    expect(panes.length).toBe(4);
    expect(panes.every((r) => r.item.kind === "pane")).toBe(true);
    const workspaces = rankItems(items, "w:a");
    expect(workspaces.map((r) => r.item.kind)).toEqual(["workspace"]);
  });

  it("drops what does not match", () => {
    expect(rankItems(items, "zzzz")).toEqual([]);
  });

  it("returns the matched positions for highlighting", () => {
    const [first] = rankItems(items, "lead");
    expect(first.positions).toEqual([0, 1, 2, 3]);
  });
});

describe("new panes elsewhere", () => {
  it("offers a pane in each distro, and one back on Windows, once WSL is there", () => {
    const withWsl = buildItems(null, SHIPPED_KEYMAP, ["windows", "wsl:Ubuntu"]);
    const places = withWsl.filter((item) => item.kind === "runtime");
    expect(places.map((item) => item.title)).toEqual(["New pane on Windows", "New pane in Ubuntu (WSL)"]);
    expect(places.map((item) => item.kind === "runtime" && item.runtime)).toEqual(["windows", "wsl:Ubuntu"]);
  });

  it("offers none on a machine with nowhere else to run", () => {
    const windowsOnly = buildItems(null, SHIPPED_KEYMAP, ["windows"]);
    expect(windowsOnly.some((item) => item.kind === "runtime")).toBe(false);
    expect(buildItems(null, SHIPPED_KEYMAP).some((item) => item.kind === "runtime")).toBe(false);
  });
});
