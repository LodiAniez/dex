import { useSyncExternalStore } from "react";
import { dismissNotice, getNotices, subscribeNotices } from "../platform/notices";

/** Transient errors, each with the daemon's repair string. */
export function NoticeBar() {
  const notices = useSyncExternalStore(subscribeNotices, getNotices);
  if (notices.length === 0) return null;
  return (
    <div className="notice-bar" role="status">
      {notices.map((notice) => (
        <div key={notice.id} className="notice">
          <div>
            <div>{notice.message}</div>
            {notice.repair && <div className="notice-repair">{notice.repair}</div>}
          </div>
          <button type="button" className="icon-button" aria-label="Dismiss" onClick={() => dismissNotice(notice.id)}>
            {""}
          </button>
        </div>
      ))}
    </div>
  );
}
