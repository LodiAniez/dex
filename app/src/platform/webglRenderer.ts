import { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";

/** A pane's terminal and the WebGL renderer it has, if any. */
export interface Rendered {
  paneId: string;
  term: Terminal;
  webgl: WebglAddon | null;
}

/**
 * Gives a shown terminal a WebGL renderer, unless it has one. Only shown
 * terminals get one: Chromium caps live WebGL contexts per page, so a
 * detached terminal's is disposed (`terminalRegistry.ts`).
 */
export function loadWebgl(entry: Rendered): void {
  if (entry.webgl) return;
  try {
    const webgl = new WebglAddon();
    // Context loss is handled like a detach: drop the addon; the next attach loads a new one.
    webgl.onContextLoss(() => {
      webgl.dispose();
      if (entry.webgl === webgl) entry.webgl = null;
    });
    entry.term.loadAddon(webgl);
    entry.webgl = webgl;
  } catch (err) {
    // No WebGL: xterm.js keeps rendering with its DOM renderer, just slower.
    console.warn(`WebGL renderer unavailable for pane ${entry.paneId}`, err);
  }
}
