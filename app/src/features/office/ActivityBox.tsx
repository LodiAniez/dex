import { useState } from "react";
import { isMinimized, newest, rememberChoice, storedChoice, unseen, unseenLabel } from "./chatBox";
import type { ChatLine } from "./phrasing";

/** Lines of chat the map has room for. */
export const CHAT_LINES = 3;

/** The box's place on the map: bottom left, under the break room. */
const BOX = { x: 26, fromBottom: 166, width: 360, height: 112 } as const;
const PILL_HEIGHT = 38;

interface Props {
  /** The last few things that happened, as the office would say them. */
  chat: readonly ChatLine[];
  /** The map's height, in its own units. */
  mapHeight: number;
}

/**
 * The last few things that happened, bottom left of the floor. Minimized it is
 * a button in the same corner, counting what has happened since.
 */
export function ActivityBox({ chat, mapHeight }: Props) {
  const [minimized, setMinimized] = useState(() => isMinimized(storedChoice()));
  // The newest line there was when the box was minimized; null if that was before this window opened.
  const [seenUpTo, setSeenUpTo] = useState<number | null>(null);
  if (chat.length === 0) return null;

  const set = (next: boolean) => {
    setMinimized(next);
    setSeenUpTo(next ? newest(chat) : null);
    rememberChoice(next);
  };

  if (minimized) {
    const fresh = unseenLabel(unseen(chat, seenUpTo), CHAT_LINES);
    return (
      <foreignObject x={BOX.x} y={mapHeight - BOX.fromBottom + BOX.height - PILL_HEIGHT} width={BOX.width} height={PILL_HEIGHT}>
        <button type="button" className="office-chat-pill" aria-expanded={false} title="Show activity" onClick={() => set(false)}>
          <span className="office-chat-title">Activity</span>
          {fresh && (
            <span className="office-chat-badge" aria-label={`${fresh} new`}>
              {fresh}
            </span>
          )}
          <span className="office-chat-toggle" aria-hidden="true">
            ▴
          </span>
        </button>
      </foreignObject>
    );
  }

  return (
    <foreignObject x={BOX.x} y={mapHeight - BOX.fromBottom} width={BOX.width} height={BOX.height}>
      <div className="office-chat">
        <div className="office-chat-head">
          <span className="office-chat-title">Activity</span>
          <button type="button" className="office-chat-toggle" aria-expanded title="Minimize" aria-label="Minimize activity" onClick={() => set(true)}>
            ▾
          </button>
        </div>
        {chat.map((line) => (
          <span key={line.seq} className={`office-chat-line ${line.tone}`}>
            <span className="who">{line.who}</span> {line.text}
          </span>
        ))}
      </div>
    </foreignObject>
  );
}
