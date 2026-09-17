import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "./office.css";
import "./map.css";
import { useState } from "react";
import { useOfficeSettings } from "../../platform/config";
import { useWorkspaces } from "../workspaces";
import { CardsFloor } from "./CardsFloor";
import { headcount, hrNote } from "./floor";
import { HireDialog } from "./HireDialog";
import { MapFloor } from "./MapFloor";
import { useOffice, useScreens, type Employee } from "./officeStore";
import { chooseView, rememberView, storedView, type OfficeViewKind } from "./officeView";
import { PANEL_SCREEN_LINES, WorkPanel } from "./WorkPanel";

/** Seats to draw before the daemon has said how many there are: its own default. */
const DEFAULT_SEATS = 6;
/** Lines of an agent's screen that fit on a card. */
const CARD_LINES = 2;

/**
 * The workspace's agents as a team in an office: who is here, what each was
 * asked to do, and what is on their screen. A projection of what the daemon
 * already tracks — nothing here changes an agent except what a human clicks.
 */
export function OfficePane({ workspaceId }: { workspaceId: string }) {
  const settings = useOfficeSettings();
  const maxConcurrent = settings?.maxConcurrent ?? DEFAULT_SEATS;
  const office = useOffice(workspaceId, maxConcurrent);
  const workspace = useWorkspaces()?.workspaces.find((ws) => ws.id === workspaceId);
  // One poll serves both: the panel wants more lines than a card shows.
  const screens = useScreens(office.employees, PANEL_SCREEN_LINES);
  // What the owner clicks wins over their config from then on, in this pane
  // and the next; until they click, the config decides, even if it changes.
  const [clicked, setClicked] = useState<string | null>(storedView);
  const view = chooseView(clicked, settings?.officeView);
  const [pickedId, setPickedId] = useState<string | null>(null);
  const [hiring, setHiring] = useState(false);
  const [refused, setRefused] = useState(false);

  const seats = headcount(office.employees.length, maxConcurrent);
  const note = hrNote(seats, office.pods, refused);
  // Whoever is picked may leave while their panel is open; it closes with them.
  const picked = office.employees.find((employee) => employee.agent.id === pickedId);

  const choose = (next: OfficeViewKind) => {
    rememberView(next);
    setClicked(next);
  };
  const pick = (employee: Employee) => setPickedId(employee.agent.id);
  const hire = () => {
    setRefused(false);
    setHiring(true);
  };

  return (
    <div className="office">
      {view === "cards" ? (
        <CardsFloor office={office} seats={seats} hrNote={note} cardLines={CARD_LINES} screens={screens} onPick={pick} onHire={hire} />
      ) : (
        <MapFloor office={office} seats={seats} hrNote={note} onPick={pick} onHire={hire} />
      )}
      {picked && (
        <WorkPanel
          workspaceId={workspaceId}
          employee={picked}
          staff={office.employees}
          screen={screens.get(picked.agent.id) ?? []}
          onClose={() => setPickedId(null)}
        />
      )}
      {hiring && workspace && (
        <HireDialog workspace={workspace} onClose={() => setHiring(false)} onFreeze={() => setRefused(true)} />
      )}
      <div className="office-toolbar">
        <span className="office-count">
          headcount {seats.used}/{seats.max}
        </span>
        <span className="office-seg" role="group" aria-label="Office view">
          <button type="button" aria-pressed={view === "cards"} onClick={() => choose("cards")}>
            Cards
          </button>
          <button type="button" aria-pressed={view === "office"} onClick={() => choose("office")}>
            Office
          </button>
        </span>
      </div>
    </div>
  );
}
