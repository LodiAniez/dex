import { describe, expect, it } from "vitest";
import { distroOf, runtimeLabel, terminalChoices } from "./runtimes";

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
    expect(terminalChoices({ terminal: "windows", runtimes: ["windows", "wsl:Ubuntu"] })).toEqual([
      { value: "windows", label: "PowerShell" },
      { value: "wsl:Ubuntu", label: "Ubuntu (WSL)" },
    ]);
  });

  it("offers none where Windows is all there is", () => {
    expect(terminalChoices({ terminal: "windows", runtimes: ["windows"] })).toEqual([]);
  });

  it("keeps a chosen distro that has gone, so the owner can choose away from it", () => {
    expect(terminalChoices({ terminal: "wsl:Ubuntu", runtimes: ["windows"] })).toEqual([
      { value: "windows", label: "PowerShell" },
      { value: "wsl:Ubuntu", label: "Ubuntu (WSL) - not installed" },
    ]);
  });
});
