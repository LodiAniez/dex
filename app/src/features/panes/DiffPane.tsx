import { useEffect, useMemo, useState } from "react";
import "./panes.css";
import { request } from "../../platform/daemon";
import { onDaemonChange } from "../../platform/events";
import type { RepoDiff } from "../../platform/generated/RepoDiff";
import { parseDiff } from "./diff";

/**
 * How often to ask git again. A diff of a working tree is cheap for git and
 * the pane is watched while agents edit, so a few seconds is about right;
 * changes made through Dex itself (worktrees, repos) also refresh it at once.
 */
const POLL_MS = 3000;

/** A repository's uncommitted changes, unstaged or staged (PRD §8, §13). */
export function DiffPane({ cwd }: { cwd: string }) {
  const [staged, setStaged] = useState(false);
  const [diff, setDiff] = useState<RepoDiff | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const refresh = async () => {
      try {
        const next = await request<RepoDiff>("repo.diff", { path: cwd, staged });
        if (!alive) return;
        setDiff((before) => (before && before.text === next.text && before.branch === next.branch ? before : next));
        setError(null);
      } catch (err) {
        if (alive) setError(err instanceof Error ? err.message : String(err));
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    const unlisten = onDaemonChange("repos", () => void refresh());
    return () => {
      alive = false;
      window.clearInterval(timer);
      void unlisten.then((stop) => stop());
    };
  }, [cwd, staged]);

  const files = useMemo(() => (diff ? parseDiff(diff.text) : []), [diff]);
  const added = files.reduce((n, f) => n + f.added, 0);
  const removed = files.reduce((n, f) => n + f.removed, 0);

  return (
    <div className="doc-pane diff-pane">
      <div className="diff-bar">
        <span className="diff-branch">{diff?.branch ?? (diff ? "detached HEAD" : "")}</span>
        <span className="diff-counts">
          {files.length > 0 && (
            <>
              {files.length} {files.length === 1 ? "file" : "files"} <span className="diff-plus">+{added}</span>{" "}
              <span className="diff-minus">−{removed}</span>
            </>
          )}
        </span>
        <span className="diff-toggle" role="tablist">
          <button type="button" role="tab" aria-selected={!staged} className={staged ? "" : "on"} onClick={() => setStaged(false)}>
            unstaged
          </button>
          <button type="button" role="tab" aria-selected={staged} className={staged ? "on" : ""} onClick={() => setStaged(true)}>
            staged
          </button>
        </span>
      </div>
      {error && <div className="doc-error">{error}</div>}
      {diff && files.length === 0 && !error && (
        <p className="doc-empty">No {staged ? "staged" : "unstaged"} changes.</p>
      )}
      <div className="diff-body">
        {files.map((file, i) => (
          <section key={`${file.path}-${i}`} className="diff-file">
            {file.path && (
              <header className="diff-file-name">
                {file.path}
                <span className="diff-counts">
                  <span className="diff-plus">+{file.added}</span> <span className="diff-minus">−{file.removed}</span>
                </span>
              </header>
            )}
            <pre>
              {file.lines
                .filter((line) => line.kind !== "file")
                .map((line, j) => (
                  <div key={j} className={`diff-line diff-${line.kind}`}>
                    {line.text || " "}
                  </div>
                ))}
            </pre>
          </section>
        ))}
      </div>
      {diff?.truncated && <p className="doc-note">Showing the first two megabytes; the diff is longer.</p>}
    </div>
  );
}
