import { useEffect, useState } from "react";
import { request } from "./daemon";
import type { RuntimeList } from "./generated/RuntimeList";

/**
 * Where a pane's shell runs: `windows`, or `wsl:<distro>` inside WSL. The
 * daemon lists what this machine has (`pane.runtimes`); a new pane runs where
 * the pane it splits from does, unless told otherwise.
 */

/** The distro of a WSL runtime; null for Windows or no runtime. */
export function distroOf(runtime: string | undefined): string | null {
  return runtime?.startsWith("wsl:") ? runtime.slice(4) : null;
}

/** A runtime as the owner reads it: "Windows", "Ubuntu (WSL)". */
export function runtimeLabel(runtime: string): string {
  if (runtime === "windows") return "Windows";
  const distro = distroOf(runtime);
  return distro ? `${distro} (WSL)` : runtime;
}

let cached: Promise<string[]> | null = null;

/**
 * Where a shell can run on this machine, Windows first. Asked once per window:
 * it costs a `wsl.exe` run, and distros are not installed in the middle of a
 * session often enough to watch for.
 */
export function useRuntimes(): string[] {
  const [runtimes, setRuntimes] = useState<string[]>(["windows"]);
  useEffect(() => {
    let live = true;
    cached ??= request<RuntimeList>("pane.runtimes", {}).then(
      (list) => list.runtimes,
      () => {
        cached = null;
        return ["windows"];
      },
    );
    void cached.then((list) => live && setRuntimes(list));
    return () => {
      live = false;
    };
  }, []);
  return runtimes;
}
