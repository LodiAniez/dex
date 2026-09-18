import { describe, expect, it } from "vitest";
import { distroOf, runtimeLabel } from "./runtimes";

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
