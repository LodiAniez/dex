/**
 * The workspace's activity, said the way an office would say it: by name, and
 * in terms of what it means to the person watching. Reads the same events the
 * activity pane shows, which stays the plain record; this is the gossip.
 */

export interface ChatLine {
  seq: number;
  who: string;
  text: string;
  /** How it should look: a hire is good news, a raised hand asks for you. */
  tone: "plain" | "good" | "asks" | "bad";
}

interface Event {
  seq: number;
  kind: string;
  agent_id: string | null;
  /** The author's label, as the daemon resolved it. */
  author: string | null;
  key: string | null;
  body: string;
}

/** Who is who: office names by agent id, and by the label agents are messaged by. */
export interface Who {
  names: ReadonlyMap<string, string>;
  labels: ReadonlyMap<string, string>;
}

/**
 * The daemon writes a hire as "<hirer> started an agent for: <brief>", where
 * the hirer is an agent's label, or "you". The event itself belongs to the
 * agent that was hired.
 */
const HIRED = /^(.*?) started an agent for: (.*)$/s;
/** And a status change as "<label> is <status>", with the reason in brackets if there is one. */
const STATUS = / is (\w+)(?: \((.*)\))?$/;
const MEMO = /^message to (.+)$/;

const STATUS_TEXT: Record<string, { text: string; tone: ChatLine["tone"] }> = {
  waiting: { text: "raised a hand — needs you", tone: "asks" },
  running: { text: "back to work", tone: "plain" },
  idle: { text: "finished, waiting for mail", tone: "plain" },
  error: { text: "stopped", tone: "bad" },
  unknown: { text: "has gone quiet", tone: "bad" },
  dead: { text: "left the office", tone: "plain" },
};

export function phrase(event: Event, who: Who): ChatLine {
  const author = event.agent_id === null ? "you" : (who.names.get(event.agent_id) ?? event.author ?? "someone");
  const line = (whom: string, text: string, tone: ChatLine["tone"] = "plain"): ChatLine => ({ seq: event.seq, who: whom, text, tone });

  switch (event.kind) {
    case "spawn": {
      const [, hirer, brief] = HIRED.exec(event.body) ?? [];
      if (hirer === undefined) return line(author, event.body);
      return line("HR", `hired ${author} for ${who.labels.get(hirer) ?? hirer}: ${brief}`, "good");
    }
    case "status": {
      const [, status, why] = STATUS.exec(event.body) ?? [];
      const said = status ? STATUS_TEXT[status] : undefined;
      return said ? line(author, why ? `${said.text} (${why})` : said.text, said.tone) : line(author, event.body);
    }
    case "message": {
      const label = MEMO.exec(event.body)?.[1];
      if (label === undefined) return line(author, event.body);
      const name = who.labels.get(label);
      // The label stays: it is what anyone would have to type to message them.
      return line(author, `sent a memo to ${name ? `${name} (${label})` : label}`);
    }
    case "write":
      return line(author, event.key ? `wrote ${event.key}` : event.body);
    case "delete":
      return line(author, event.key ? `deleted ${event.key}` : event.body);
    default:
      return line(author, event.body);
  }
}

/** The newest `count` events as chat, oldest first. */
export function chatLines(events: readonly Event[] | undefined, who: Who, count: number): ChatLine[] {
  return (events ?? []).slice(-count).map((event) => phrase(event, who));
}
