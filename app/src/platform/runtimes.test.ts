import { describe, expect, it } from "vitest";
import { distroOf, restartChoices, runtimeLabel, switchNow, terminalChoices } from "./runtimes";

describe("runtimeLabel", () => {
  it("names a terminal the way the owner thinks of it", () => {
    expect(runtimeLabel("windows")).toBe("PowerShell");
    expect(runtimeLabel("wsl:Ubuntu")).toBe("Ubuntu (WSL)");
    expect(runtimeLabel("wsl:Ubuntu-24.04")).toBe("Ubuntu-24.04 (WSL)");
  });

  it("shows anything else as it is, rather than guessing", () => {
    expect(runtimeLabel("mars")).toBe("mars");
  });
});

describe("distroOf", () => {
  it("is the distro of a WSL terminal, and nothing for Windows", () => {
    expect(distroOf("wsl:Ubuntu")).toBe("Ubuntu");
    expect(distroOf("windows")).toBeNull();
    expect(distroOf(undefined)).toBeNull();
  });
});

describe("terminalChoices", () => {
  it("offers every terminal there is, once there is a choice", () => {
    expect(terminalChoices({ terminal: "windows", runtimes: ["windows", "wsl:Ubuntu"], running_elsewhere: [] })).toEqual([
      { value: "windows", label: "PowerShell" },
      { value: "wsl:Ubuntu", label: "Ubuntu (WSL)" },
    ]);
  });

  it("offers none where Windows is all there is", () => {
    expect(terminalChoices({ terminal: "windows", runtimes: ["windows"], running_elsewhere: [] })).toEqual([]);
  });

  it("keeps a chosen distro that has gone, so the owner can choose away from it", () => {
    expect(terminalChoices({ terminal: "wsl:Ubuntu", runtimes: ["windows"], running_elsewhere: [] })).toEqual([
      { value: "windows", label: "PowerShell" },
      { value: "wsl:Ubuntu", label: "Ubuntu (WSL) - not installed" },
    ]);
  });
});

describe("switchNow", () => {
  const running = { spawned: true, dead: false, away: false, runtime: "windows" };

  it("restarts a running shell whose pane now runs elsewhere", () => {
    expect(switchNow(running, "wsl:Ubuntu")).toBe(true);
  });

  it("leaves a shell where its pane still runs", () => {
    expect(switchNow(running, "windows")).toBe(false);
  });

  it("leaves a shell not started yet, an exited one, and one shown in another window", () => {
    expect(switchNow({ ...running, spawned: false }, "wsl:Ubuntu")).toBe(false);
    expect(switchNow({ ...running, dead: true }, "wsl:Ubuntu")).toBe(false);
    expect(switchNow({ ...running, away: true }, "wsl:Ubuntu")).toBe(false);
  });
});

describe("restartChoices", () => {
  const pane = (id: string, busy: boolean, label: string | null = null) => ({
    pane: id,
    workspace: "api",
    label,
    cwd: "C:/src/api",
    runtime: "windows",
    busy,
  });

  it("ticks plain shells and leaves panes that may be busy for the owner to decide", () => {
    const choices = restartChoices([pane("a", false), pane("b", true, "server")], () => true);
    expect(choices.map((c) => [c.pane, c.ticked])).toEqual([
      ["a", true],
      ["b", false],
    ]);
    expect(choices[0].title).toBe("api");
    expect(choices[1].title).toBe("server");
    expect(choices[1].detail).toContain("something may be running in it");
  });

  it("offers only panes this window can restart", () => {
    expect(restartChoices([pane("a", false), pane("b", false)], (id) => id === "b").map((c) => c.pane)).toEqual(["b"]);
  });
});
