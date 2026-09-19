/**
 * How a mirror keeps hold of its pane, kept pure (`terminalMirror.ts` wires it
 * to the terminals and to app events). The pane is watched here if this window
 * holds it; otherwise the window that does is asked, and says hello at once.
 * When the pane leaves, or comes back without a word, the link looks again.
 */
import type { MirrorMessage } from "./mirrorFeed";

/** What crosses between windows: a mirror's messages, and the hello that answers a request. */
export type Wire = MirrorMessage | { kind: "hello" };

export interface LinkDeps {
  /** Watches the pane here, if this window holds it: how to stop, or null. */
  here(send: (message: MirrorMessage) => void): (() => void) | null;
  /** Whether this window holds the pane now. */
  isHere(): boolean;
  /** Asks the other windows for the pane, under this request id. */
  ask(id: string): void;
  /** Withdraws a request. */
  cancel(id: string): void;
  newId(): string;
}

export class MirrorLink {
  private stopped = false;
  private local: (() => void) | null = null;
  private asked: string | null = null;
  private greeted = false;
  /** Whether answers from other windows can be heard yet: asking before would miss them. */
  private listening = false;

  constructor(
    private readonly deps: LinkDeps,
    private readonly send: (message: MirrorMessage) => void,
  ) {}

  start(): void {
    this.attach();
  }

  /** Answers from other windows can be heard now. */
  ready(): void {
    this.listening = true;
    if (!this.stopped && this.local === null && this.asked === null) this.attach();
  }

  /** An answer from another window. */
  heard(id: string, message: Wire): void {
    if (this.stopped || id !== this.asked) return;
    // Any answer is a hello: the screen can be on the wire before it.
    this.greeted = true;
    if (message.kind !== "hello") this.fromPane(message);
  }

  /**
   * Called every second or so: the pane came back here without a word, or
   * nobody has answered - look again. Once greeted, a slow screen is waited for.
   */
  tick(): void {
    if (this.stopped) return;
    if (this.local === null && this.deps.isHere()) this.attach();
    else if (this.asked !== null && !this.greeted) this.attach();
  }

  stop(): void {
    this.stopped = true;
    this.forget();
  }

  private fromPane(message: MirrorMessage): void {
    this.send(message);
    // The pane left or closed: look for it again.
    if (message.kind === "end") this.attach();
  }

  private attach(): void {
    this.forget();
    if (this.stopped) return;
    this.local = this.deps.here((message) => this.fromPane(message));
    if (this.local !== null || !this.listening) return;
    this.asked = this.deps.newId();
    this.greeted = false;
    this.deps.ask(this.asked);
  }

  private forget(): void {
    this.local?.();
    this.local = null;
    if (this.asked !== null) this.deps.cancel(this.asked);
    this.asked = null;
  }
}
