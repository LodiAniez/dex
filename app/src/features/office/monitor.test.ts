import { describe, expect, it } from "vitest";
import { fitFont, monitorLed, promptLine } from "./monitor";

describe("the monitor's font", () => {
  // A monospace cell is about 0.6 of the font size wide and 1.2 tall.
  const cell = { width: 0.6, height: 1.2 };

  it("fits the agent's whole terminal on the screen", () => {
    const size = fitFont({ cols: 120, rows: 30 }, { width: 900, height: 500 }, cell);
    expect(120 * 0.6 * size).toBeLessThanOrEqual(900);
    expect(30 * 1.2 * size).toBeLessThanOrEqual(500);
  });

  it("is limited by whichever side runs out first", () => {
    // Wide and short: rows decide.
    expect(fitFont({ cols: 80, rows: 50 }, { width: 2000, height: 600 }, cell)).toBe(10);
    // Narrow: columns decide.
    expect(fitFont({ cols: 200, rows: 10 }, { width: 1200, height: 2000 }, cell)).toBe(10);
  });

  it("stays readable, never below the smallest size or above the largest", () => {
    expect(fitFont({ cols: 400, rows: 200 }, { width: 300, height: 200 }, cell)).toBe(6);
    expect(fitFont({ cols: 20, rows: 5 }, { width: 3000, height: 3000 }, cell)).toBe(16);
  });

  it("uses whole and half sizes, which render crisply", () => {
    const size = fitFont({ cols: 117, rows: 33 }, { width: 913, height: 517 }, cell);
    expect(size * 2).toBe(Math.floor(size * 2));
  });
});

describe("the monitor's power light", () => {
  it("shows what the agent is doing, as their status dot does", () => {
    expect(monitorLed("running")).toBe("running");
    expect(monitorLed("waiting")).toBe("waiting");
    expect(monitorLed("idle")).toBe("idle");
    // Stopped, gone, or nobody knows: the light is off.
    expect(monitorLed("error")).toBe("off");
    expect(monitorLed("dead")).toBe("off");
    expect(monitorLed("unknown")).toBe("off");
  });
});

describe("the monitor's prompt line", () => {
  const ready = { status: "idle" as const, pane_id: "p1", started: true };

  it("takes a prompt when the agent can be prompted", () => {
    expect(promptLine(ready, false)).toEqual({ live: true, hint: "Enter sends it as your turn" });
  });

  it("says why not, and takes nothing, when it cannot", () => {
    expect(promptLine({ ...ready, status: "waiting" }, false)).toEqual({ live: false, hint: "Cannot prompt: waiting for you" });
    expect(promptLine({ ...ready, started: false }, false)).toEqual({
      live: false,
      hint: "Cannot prompt: Claude Code has not started yet",
    });
  });

  it("takes nothing more while a prompt is on its way", () => {
    expect(promptLine(ready, true)).toEqual({ live: false, hint: "Sending..." });
  });
});
