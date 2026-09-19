import { Terminal } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
import { mirrorTerminal } from "../../platform/terminalMirror";
import { fitFont } from "./monitor";

/** A monospace character's width and height per pixel of font size, with room to spare. */
const CELL = { width: 0.6, height: 1.2 };
/** The smallest the text goes when stepping down to make the whole screen fit. */
const SMALLEST = 6;

/**
 * The monitor's screen: a read-only copy of the agent's terminal, live. It has
 * the terminal's own columns and rows - the output is laid out for them - and
 * its text is sized to fit the glass. It never takes a key: `onClick` sends the
 * keyboard to the prompt line below, unless text was selected, which Ctrl+C or
 * Cmd+C copies. It scrolls back like any terminal, and follows new output when
 * at the bottom.
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
    const stop = mirrorTerminal(paneId, (message) => {
      if (message.kind === "data") {
        term.write(message.text);
        return;
      }
      if (message.kind === "end") return;
      const { cols, rows } = message;
      if (message.kind === "screen") {
        grid = { cols, rows };
        term.reset();
        term.resize(cols, rows);
        refit();
        term.write(message.content);
        setLive(true);
        return;
      }
      // After what is already on its way: it was printed at the old size.
      term.write("", () => {
        grid = { cols, rows };
        term.resize(cols, rows);
        refit();
      });
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
      onKeyDown={(event) => {
        const term = termRef.current;
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "c" && term?.hasSelection()) {
          event.preventDefault();
          void navigator.clipboard.writeText(term.getSelection()).catch(() => {});
        }
      }}
    >
      <div className="monitor-term" ref={host} />
      {!live && <div className="monitor-waking">Waking the screen...</div>}
    </div>
  );
}
