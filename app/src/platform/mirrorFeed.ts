/**
 * A pane's output as a mirror sees it: the screen as it was when the mirror
 * started, then everything printed after, in order. Output that arrives while
 * the screen is being read is kept and sent after it, so nothing is lost and
 * nothing is shown twice. The mirror only watches; it never writes to the pane.
 */

/** What a mirror is sent. */
export type MirrorMessage =
  | { kind: "screen"; content: string; cols: number; rows: number }
  | { kind: "data"; text: string }
  | { kind: "resize"; cols: number; rows: number }
  /** The pane left: it moved to another window or closed. A mirror asks again. */
  | { kind: "end" };

type Pending = { kind: "data"; text: string } | { kind: "resize"; cols: number; rows: number };

export class MirrorFeed {
  /** The most output kept while waiting for the screen: a screen that never comes must not eat memory. */
  static readonly MAX_BACKLOG = 256 * 1024;

  private backlog: Pending[] | null = [];
  private backlogSize = 0;
  private stopped = false;
  /** The latest size among output dropped from the backlog: still the pane's size. */
  private droppedSize: { cols: number; rows: number } | null = null;

  constructor(private readonly send: (message: MirrorMessage) => void) {}

  /** The screen as it was: sent first, then whatever was printed meanwhile. */
  start(content: string, cols: number, rows: number): void {
    if (this.stopped || this.backlog === null) return;
    this.send({ kind: "screen", content, cols, rows });
    if (this.droppedSize) this.send({ kind: "resize", ...this.droppedSize });
    const waiting = this.backlog;
    this.backlog = null;
    for (const message of waiting) this.send(message);
  }

  /** Output printed on the pane. */
  data(text: string): void {
    if (this.stopped) return;
    if (this.backlog === null) {
      this.send({ kind: "data", text });
      return;
    }
    const last = this.backlog.at(-1);
    if (last?.kind === "data") last.text += text;
    else this.backlog.push({ kind: "data", text });
    this.backlogSize += text.length;
    this.trim();
  }

  /** The pane's terminal changed size: the mirror must lay out as it does. */
  resize(cols: number, rows: number): void {
    if (this.stopped) return;
    if (this.backlog === null) this.send({ kind: "resize", cols, rows });
    else this.backlog.push({ kind: "resize", cols, rows });
  }

  /** The mirror closed: nothing more is sent. */
  stop(): void {
    this.stopped = true;
    this.backlog = null;
  }

  /** The pane is leaving this window: the mirror is told, once, and then nothing more. */
  end(): void {
    if (this.stopped) return;
    this.send({ kind: "end" });
    this.stop();
  }

  /** Keeps the newest output within the limit, dropping the oldest. */
  private trim(): void {
    const backlog = this.backlog;
    if (backlog === null) return;
    while (this.backlogSize > MirrorFeed.MAX_BACKLOG && backlog.length > 0) {
      const first = backlog[0];
      if (first.kind !== "data") {
        this.droppedSize = { cols: first.cols, rows: first.rows };
        backlog.shift();
        continue;
      }
      const over = this.backlogSize - MirrorFeed.MAX_BACKLOG;
      if (first.text.length <= over) {
        backlog.shift();
        this.backlogSize -= first.text.length;
      } else {
        first.text = first.text.slice(over);
        this.backlogSize -= over;
      }
    }
  }
}
