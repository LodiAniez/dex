import { Terminal } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
import { mirrorTerminal } from "../../platform/terminalMirror";
import { fitFont, screenTakes } from "./monitor";

/** A monospace character's width and height per pixel of font size, with room to spare. */
const CELL = { width: 0.6, height: 1.2 };
/** The smallest the text goes when stepping down to make the whole screen fit. */
const SMALLEST = 6;

/**
 * The monitor's screen: a read-only copy of the agent's terminal, live. It has
 * the terminal's own columns and rows - the output is laid out for them - and
 * its text is sized to fit the glass. It never takes a key: `onClick` sends the
 * keyboard to the prompt line below, unless text was selected, which Ctrl+C or
 * Cmd+C copies (the browser's copy, which xterm fills with the selection). It
 * scrolls back like any terminal, and follows new output when at the bottom.
 */
export function MonitorScreen({ paneId, onClick }: { paneId: string; onClick: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const [live, setLive] = useState(false);

  useEffect(() => {
    const boxElement = box.current;
    const hostElement = host.current;
    if (!boxElement || !hostElement) return;
    const term = new Terminal({
      disableStdin: true,
      cursorBlink: false,
      fontFamily: '"Cascadia Mono", Consolas, Menlo, monospace',
      fontSize: 12,
      scrollback: 1000,
      theme: { background: "#07090d", foreground: "#d7dae0" },
    });
    termRef.current = term;
    term.open(hostElement);
    // Not a stop for Tab: the prompt line and the close button are the monitor's.
    term.textarea?.setAttribute("tabindex", "-1");
    // xterm takes every key its textarea gets, even with input off.
    term.attachCustomKeyEventHandler((event) => screenTakes(event, term.hasSelection()));
    let grid = { cols: term.cols, rows: term.rows };
    let frame = 0;
    // The estimate can be a pixel out once xterm rounds its cells: after each
    // redraw, measure, and step down until the whole screen - newest line
    // included - shows.
    const settle = () => {
      frame = 0;
      const screen = hostElement.querySelector<HTMLElement>(".xterm-screen");
      const size = term.options.fontSize ?? SMALLEST;
      if (!screen || size <= SMALLEST) return;
      if (screen.offsetHeight > boxElement.clientHeight || screen.offsetWidth > boxElement.clientWidth) {
        term.options.fontSize = size - 0.5;
        frame = requestAnimationFrame(settle);
      }
    };
    const refit = () => {
      term.options.fontSize = fitFont(grid, { width: boxElement.clientWidth, height: boxElement.clientHeight }, CELL);
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(settle);
    };
    // Each fresh screen starts a new stream; what is still queued from an
    // older one must not draw over it, nor size it.
    let stream = 0;
    const stop = mirrorTerminal(paneId, (message) => {
      if (message.kind === "data") {
        term.write(message.text);
        return;
      }
      if (message.kind === "end") {
        setLive(false); // Looking for the pane again: say so, not a frozen frame.
        return;
      }
      const { cols, rows } = message;
      const mine = message.kind === "screen" ? ++stream : stream;
      // In order, behind what is already on its way (it was printed at the old
      // size): the size first, then - for a fresh screen - ESC c, which wipes
      // whatever of the old stream was drawn before it, and the screen itself.
      term.write("", () => {
        if (mine !== stream) return;
        grid = { cols, rows };
        term.resize(cols, rows);
        refit();
      });
      if (message.kind === "screen") {
        term.write(`\x1bc${message.content}`, () => {
          if (mine === stream) setLive(true);
        });
      }
    });
    const observer = new ResizeObserver(refit);
    observer.observe(boxElement);
    return () => {
      stop();
      cancelAnimationFrame(frame);
      observer.disconnect();
      termRef.current = null;
      term.dispose();
    };
  }, [paneId]);

  return (
    <div
      className="monitor-screen"
      ref={box}
      onClick={() => {
        if (!termRef.current?.hasSelection()) onClick();
      }}
    >
      <div className="monitor-term" ref={host} />
      {!live && <div className="monitor-waking">Looking for their screen...</div>}
    </div>
  );
}
