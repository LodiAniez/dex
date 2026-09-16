import { useEffect, useMemo, useState } from "react";
import "./panes.css";
import { request } from "../../platform/daemon";
import type { PaneContent } from "../../platform/generated/PaneContent";
import { parseMarkdown } from "./markdown";
import { Markdown } from "./MarkdownView";

/**
 * How often to look for changes. The daemon reads the file each time; that is
 * cheap for the note-sized files this pane is for, and it means an agent's
 * edit shows up within a couple of seconds with no watcher to keep alive.
 */
const POLL_MS = 2000;

/** A rendered markdown file, following it as it changes (PRD §13). */
export function MarkdownPane({ paneId }: { paneId: string }) {
  const [content, setContent] = useState<PaneContent | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const refresh = async () => {
      try {
        const next = await request<PaneContent>("pane.content", { pane: paneId });
        if (!alive) return;
        // Only re-render on a real change; the parser is cheap but not free.
        setContent((before) => (before && before.text === next.text ? before : next));
        setError(null);
      } catch (err) {
        if (alive) setError(err instanceof Error ? err.message : String(err));
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [paneId]);

  const blocks = useMemo(() => (content ? parseMarkdown(content.text) : []), [content]);

  return (
    <div className="doc-pane">
      {error && <div className="doc-error">{error}</div>}
      {content && blocks.length === 0 && !error && <p className="doc-empty">This file is empty.</p>}
      <Markdown blocks={blocks} />
      {content?.truncated && <p className="doc-note">Showing the first megabyte; the file is longer.</p>}
    </div>
  );
}
