/** Workspace colors offered by the picker. Mirrors `PALETTE` in dex-core's workspace/logic.rs. */
export const PALETTE = ["#4f8cff", "#3ecf8e", "#f5a524", "#f25f5c", "#a970ff", "#2ec4d6", "#f178b6", "#8a93a6"];

/** The color the daemon will give the next workspace when none is chosen. */
export function nextPaletteColor(existing: number): string {
  return PALETTE[existing % PALETTE.length];
}
