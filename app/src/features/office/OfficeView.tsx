import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "./office.css";
import "./map.css";
import "./antics.css";
import "./shouting.css";
import "./speech.css";
import "./panel.css";
import { useEffect, useMemo, useRef, useState } from "react";
import { useUiSettings } from "../../platform/config";
import { useActivity, watchActivity } from "../activity";
import { useWorkspaces } from "../workspaces";
import { AnnounceDialog } from "./Announce";
import { headcount, hrNote } from "./floor";
import { HireDialog } from "./HireDialog";
import { MapFloor } from "./MapFloor";
import { MonitorDialog } from "./MonitorDialog";
import { useOffice, useScreen, type Employee } from "./officeStore";
import { knownAs } from "./persona";
import { CHAT_LINES } from "./ActivityBox";
import { chatLines } from "./phrasing";
import { PANEL_SCREEN_LINES, WorkPanel } from "./WorkPanel";

interface Props {
  workspaceId: string;
  /** Leaves the view for the terminals, on this pane. */
  onGoToPane: (paneId: string) => void;
}

/** Seats to draw before the daemon has said how many there are: its own default. */
const DEFAULT_SEATS = 10;

/**
 * The workspace's agents as a team in an office: who is here, what each was
 * asked to do, and what is on their screen. A projection of what the daemon
 * already tracks — nothing here changes an agent except what a human clicks.
 */
export function OfficeView({ workspaceId, onGoToPane }: Props) {
  const settings = useUiSettings();
  const maxConcurrent = settings?.maxConcurrent ?? DEFAULT_SEATS;
  const office = useOffice(workspaceId, maxConcurrent);
  const workspace = useWorkspaces()?.workspaces.find((ws) => ws.id === workspaceId);
  const [pickedId, setPickedId] = useState<string | null>(null);
  const [hiring, setHiring] = useState(false);
  const [announcing, setAnnouncing] = useState(false);
  const [refused, setRefused] = useState(false);
  // Whose screen is open on the monitor: one at a time.
  const [monitorId, setMonitorId] = useState<string | null>(null);

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
  // Only whoever's panel is open has their screen read.
  const screen = useScreen(picked?.agent.pane_id, PANEL_SCREEN_LINES);
  // And whoever is on the monitor, likewise: it closes if they leave.
  const monitored = office.employees.find((employee) => employee.agent.id === monitorId);

  const pick = (employee: Employee) => setPickedId(employee.agent.id);
  const hire = () => {
    setRefused(false);
    setHiring(true);
  };
  // When HR's form goes, so does whatever in it had focus; the office takes it
  // back rather than leave it on the page body.
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!hiring && !announcing) root.current?.focus();
  }, [hiring, announcing]);

  return (
    <div className="office" ref={root} tabIndex={-1}>
      <div className="office-top">
        {workspace && (
          <div className="office-title">
            <span className="office-title-dot" style={{ background: workspace.color ?? "#d85a30" }} />
            {workspace.name} — the office
            <span className="office-title-dex">dex</span>
          </div>
        )}
      </div>
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
      {picked && (
        <WorkPanel
          workspaceId={workspaceId}
          employee={picked}
          staff={office.employees}
          who={who}
          screen={screen}
          onGoToPane={onGoToPane}
          onExpand={() => setMonitorId(picked.agent.id)}
          onClose={() => setPickedId(null)}
        />
      )}
      {monitored && <MonitorDialog employee={monitored} onClose={() => setMonitorId(null)} />}
      {announcing && <AnnounceDialog staff={office.employees} onClose={() => setAnnouncing(false)} />}
      {hiring && workspace && (
        <HireDialog workspace={workspace} onClose={() => setHiring(false)} onFreeze={() => setRefused(true)} />
      )}
    </div>
  );
}
