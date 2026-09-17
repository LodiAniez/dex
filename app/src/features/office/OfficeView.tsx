import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "./office.css";
import "./cards.css";
import "./map.css";
import "./panel.css";
import { useEffect, useMemo, useRef, useState } from "react";
import { useUiSettings } from "../../platform/config";
import { useActivity, watchActivity } from "../activity";
import { useWorkspaces } from "../workspaces";
import { AnnounceDialog, Horn } from "./Announce";
import { CardsFloor } from "./CardsFloor";
import { headcount, hrNote } from "./floor";
import { HireDialog } from "./HireDialog";
import { MapFloor } from "./MapFloor";
import { OutputModal } from "./OutputModal";
import { useOffice, useScreens, type Employee } from "./officeStore";
import { knownAs } from "./persona";
import { chatLines } from "./phrasing";
import { PANEL_SCREEN_LINES, WorkPanel } from "./WorkPanel";

interface Props {
  workspaceId: string;
  view: "cards" | "office";
  /** Leaves the view for the terminals, on this pane. */
  onGoToPane: (paneId: string) => void;
}

/** Seats to draw before the daemon has said how many there are: its own default. */
const DEFAULT_SEATS = 10;
/** Lines of an agent's screen a card asks for: the card is their screen, and shows as many as fit. */
const CARD_LINES = 30;
/** Lines of chat the map has room for. */
const CHAT_LINES = 3;

/**
 * The workspace's agents as a team in an office: who is here, what each was
 * asked to do, and what is on their screen. A projection of what the daemon
 * already tracks — nothing here changes an agent except what a human clicks.
 *
 * Which of its two faces to show is the caller's business: the views are
 * chosen for the whole workspace, from the title bar.
 */
export function OfficeView({ workspaceId, view, onGoToPane }: Props) {
  const settings = useUiSettings();
  const maxConcurrent = settings?.maxConcurrent ?? DEFAULT_SEATS;
  const office = useOffice(workspaceId, maxConcurrent);
  const workspace = useWorkspaces()?.workspaces.find((ws) => ws.id === workspaceId);
  // One poll serves both: the panel wants more lines than a card shows.
  const screens = useScreens(office.employees, Math.max(PANEL_SCREEN_LINES, CARD_LINES));
  const [pickedId, setPickedId] = useState<string | null>(null);
  const [hiring, setHiring] = useState(false);
  const [announcing, setAnnouncing] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [refused, setRefused] = useState(false);

  // The same events the activity pane shows, retold by name for the map.
  useEffect(() => watchActivity(workspaceId), [workspaceId]);
  const log = useActivity(workspaceId);
  const who = useMemo(() => {
    const names = new Map(office.employees.map(({ agent, persona }) => [agent.id, persona.name]));
    const labels = new Map<string, string>();
    // Every name the daemon may have used for someone in an event.
    for (const { agent, persona } of office.employees) {
      for (const label of knownAs(agent, workspace?.panes)) labels.set(label, persona.name);
    }
    return { names, labels };
  }, [office.employees, workspace]);
  const chat = useMemo(() => chatLines(log?.events, who, CHAT_LINES), [log, who]);

  const seats = headcount(office.employees.length, maxConcurrent);
  const note = hrNote(seats, office.pods, refused);
  // Whoever is picked may leave while their panel is open; it closes with them.
  const picked = office.employees.find((employee) => employee.agent.id === pickedId);

  const pick = (employee: Employee) => setPickedId(employee.agent.id);
  // Whoever's output is expanded may leave while it is open; it closes with them.
  const expanded = office.employees.find((employee) => employee.agent.id === expandedId);
  const hire = () => {
    setRefused(false);
    setHiring(true);
  };
  // When HR's form goes, so does whatever in it had focus; the office takes it
  // back rather than leave it on the page body.
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!hiring && !announcing && !expandedId) root.current?.focus();
  }, [hiring, announcing, expandedId]);

  return (
    <div className={`office ${view}`} ref={root} tabIndex={-1}>
      <div className="office-top">
        {workspace && (
          <div className="office-title">
            <span className="office-title-dot" style={{ background: workspace.color ?? "#d85a30" }} />
            {workspace.name} — the office
            <span className="office-title-dex">dex</span>
          </div>
        )}
        {/* In the cards view the speaker sits beside the name; on the map it is on the wall. */}
        {view === "cards" && (
          <button type="button" className="office-announce" disabled={office.employees.length === 0} onClick={() => setAnnouncing(true)}>
            <Horn size={18} /> Announce
          </button>
        )}
      </div>
      {view === "cards" ? (
        <CardsFloor office={office} seats={seats} hrNote={note} cardLines={CARD_LINES} screens={screens} onGoToPane={onGoToPane} onDetails={pick} onExpand={(employee) => setExpandedId(employee.agent.id)} onHire={hire} />
      ) : (
        <MapFloor
          workspaceId={workspaceId}
          office={office}
          seats={seats}
          hrNote={note}
          chat={chat}
          events={log?.events}
          onPick={pick}
          onHire={hire}
          onAnnounce={() => setAnnouncing(true)}
        />
      )}
      {picked && (
        <WorkPanel
          workspaceId={workspaceId}
          employee={picked}
          staff={office.employees}
          who={who}
          screen={(screens.get(picked.agent.id) ?? []).slice(-PANEL_SCREEN_LINES)}
          onGoToPane={onGoToPane}
          onClose={() => setPickedId(null)}
        />
      )}
      {expanded && <OutputModal employee={expanded} onGoToPane={onGoToPane} onClose={() => setExpandedId(null)} />}
      {announcing && <AnnounceDialog staff={office.employees} onClose={() => setAnnouncing(false)} />}
      {hiring && workspace && (
        <HireDialog workspace={workspace} onClose={() => setHiring(false)} onFreeze={() => setRefused(true)} />
      )}
    </div>
  );
}
