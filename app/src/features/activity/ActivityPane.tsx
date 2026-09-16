import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import type { EventView } from "../../platform/generated/EventView";
import { showError } from "../../platform/notices";
import { clearEvents, deleteEvent, useActivity, watchActivity } from "./activityStore";

/**
 * The workspace's live event stream (docs/prd.md §10.4): what every agent here
 * has been doing, newest last, filterable by agent. This is how a human follows
 * four agents at once.
 */
export function ActivityPane({ workspaceId }: { workspaceId: string }) {
  useEffect(() => watchActivity(workspaceId), [workspaceId]);
  const log = useActivity(workspaceId);
  const [author, setAuthor] = useState<string | null>(null);

  const authors = useMemo(() => {
    const names = new Set<string>();
    for (const event of log?.events ?? []) names.add(event.author ?? HUMAN);
    return [...names].sort();
  }, [log]);

  // A filter on an agent that has since gone quiet would hide everything with
  // no way back, so it falls back to showing all.
  const active = author && authors.includes(author) ? author : null;
  const events = (log?.events ?? []).filter((event) => !active || (event.author ?? HUMAN) === active);

  /** Clears after a confirmation: this removes what other people can see too. */
  const clear = async (scope: "ended" | "all") => {
    const question =
      scope === "all"
        ? "Remove every event in this workspace's activity?"
        : "Remove the activity of agents that have ended? What live agents and you have done stays.";
    if (!(await ask(question, { title: "Clear activity", kind: "warning" }))) return;
    try {
      await clearEvents(workspaceId, scope);
    } catch (err) {
      showError(err);
    }
  };

  const hasEvents = (log?.events.length ?? 0) > 0;
  return (
    <div className="activity">
      {(authors.length > 1 || hasEvents) && (
        <div className="activity-filters">
          {authors.length > 1 && (
            <>
              <FilterChip label="everyone" on={!active} onClick={() => setAuthor(null)} />
              {authors.map((name) => (
                <FilterChip key={name} label={name} on={active === name} onClick={() => setAuthor(name)} />
              ))}
            </>
          )}
          {hasEvents && (
            <span className="activity-clear">
              <button type="button" className="activity-chip" title="Remove events from agents that have ended" onClick={() => void clear("ended")}>
                clear ended
              </button>
              <button type="button" className="activity-chip" title="Remove every event" onClick={() => void clear("all")}>
                clear all
              </button>
            </span>
          )}
        </div>
      )}
      <ol className="activity-list">
        {events.map((event) => (
          <Line key={event.seq} event={event} onDelete={() => void deleteEvent(workspaceId, event.seq).catch(showError)} />
        ))}
      </ol>
      {log && events.length === 0 && (
        <p className="activity-empty">
          Nothing yet. Notes, writes and messages from the agents in this workspace appear here.
        </p>
      )}
    </div>
  );
}

/** Events with no agent behind them were the human, through the CLI. */
const HUMAN = "you";

function FilterChip({ label, on, onClick }: { label: string; on: boolean; onClick: () => void }) {
  return (
    <button type="button" className={`activity-chip${on ? " on" : ""}`} onClick={onClick}>
      {label}
    </button>
  );
}

function Line({ event, onDelete }: { event: EventView; onDelete: () => void }) {
  return (
    <li className="activity-line">
      <time className="activity-time" dateTime={new Date(event.created_at).toISOString()}>
        {clock(event.created_at)}
      </time>
      <span className={`activity-kind kind-${event.kind}`}>{event.kind}</span>
      <span className="activity-author">{event.author ?? HUMAN}</span>
      <span className="activity-body">{event.body}</span>
      {/* One event is cheap to remove and cheap to have removed by mistake, so no confirmation. */}
      <button type="button" className="activity-delete" title={`Remove event ${event.seq}`} aria-label="Remove event" onClick={onDelete}>
        ×
      </button>
    </li>
  );
}

/** Wall-clock time, which is what a human watching live wants. */
function clock(millis: number): string {
  return new Date(millis).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}
