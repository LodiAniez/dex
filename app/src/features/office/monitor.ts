/**
 * The monitor: an agent's screen, opened large from their work panel. What it
 * decides is kept here, pure - how big its text is, what its power light shows,
 * and whether its prompt line takes a prompt.
 */

import type { AgentStatus } from "../../platform/generated/AgentStatus";
import { whyNoPrompt } from "./prompt";

/** The smallest and largest font the monitor uses, in pixels. */
const SMALLEST = 6;
const LARGEST = 16;

/**
 * The font size that fits the agent's whole terminal on the monitor's screen.
 * The mirror must have the terminal's own columns and rows - its output is laid
 * out for them - so it is the text that scales, not the grid. `cell` is a
 * character's width and height per pixel of font size.
 */
export function fitFont(
  grid: { cols: number; rows: number },
  box: { width: number; height: number },
  cell: { width: number; height: number },
): number {
  const byWidth = box.width / (grid.cols * cell.width);
  const byHeight = box.height / (grid.rows * cell.height);
  const size = Math.floor(Math.min(byWidth, byHeight) * 2) / 2;
  return Math.min(LARGEST, Math.max(SMALLEST, size));
}

/** The monitor's power light: what the agent is doing, or off when it is not there. */
export type Led = "running" | "waiting" | "idle" | "off";

export function monitorLed(status: AgentStatus): Led {
  if (status === "running" || status === "waiting" || status === "idle") return status;
  return "off";
}

/** Whether the prompt line takes a prompt, and what it says under the cursor. */
export function promptLine(
  agent: Parameters<typeof whyNoPrompt>[0],
  sending: boolean,
): { live: boolean; hint: string } {
  if (sending) return { live: false, hint: "Sending..." };
  const why = whyNoPrompt(agent);
  return why === null ? { live: true, hint: "Enter sends it as your turn" } : { live: false, hint: `Cannot prompt: ${why}` };
}
