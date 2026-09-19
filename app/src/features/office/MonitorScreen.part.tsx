import { Terminal } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
import { mirrorTerminal } from "../../platform/terminalMirror";
import { fitFont } from "./monitor";

/** A monospace character's width and height per pixel of font size, with room to spare. */
const CELL = { width: 0.6, height: 1.2 };

/**
 * The monitor's screen: a read-only copy of the agent's terminal, live. It has
 * the terminal's own columns and rows - the output is laid out for them - and
 * its text is sized to fit the glass. It never takes a key; `onClick` sends the
 * keyboard to the prompt line below.
 */
export function MonitorScreen({ paneId, onClick }: { paneId: string; onClick: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
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
    term.open(hostElement);
    let grid = { cols: term.cols, rows: term.rows };
    const refit = () => {
      const size = fitFont(grid, { width: boxElement.clientWidth, height: boxElement.clientHeight }, CELL);
      if (term.options.fontSize !== size) term.options.fontSize = size;
    };
    const stop = mirrorTerminal(paneId, (message) => {
      if (message.kind === "data") {
        term.write(message.text);
        return;
      }
      grid = { cols: message.cols, rows: message.rows };
      term.resize(message.cols, message.rows);
      refit();
      if (message.kind === "screen") {
        term.reset();
        term.write(message.content);
        setLive(true);
      }
    });
    const observer = new ResizeObserver(refit);
    observer.observe(boxElement);
    return () => {
      stop();
      observer.disconnect();
      term.dispose();
    };
  }, [paneId]);

  return (
    <div className="monitor-screen" ref={box} onClick={onClick}>
      <div className="monitor-term" ref={host} />
      {!live && <div className="monitor-waking">Waking the screen...</div>}
    </div>
  );
}
