import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "./office.css";
import { useState } from "react";
import { useOfficeSettings } from "../../platform/config";
import { showError } from "../../platform/notices";
import { focusPane } from "../workspaces";
import { CardsFloor } from "./CardsFloor";
import { headcount } from "./floor";
import { useOffice, useScreens, type Employee } from "./officeStore";
import { chooseView, rememberView, storedView, type OfficeViewKind } from "./officeView";

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
  const office = useOffice(workspaceId, settings?.maxConcurrent ?? DEFAULT_SEATS);
  const screens = useScreens(office.employees, CARD_LINES);
  // What the owner clicks wins over their config from then on, in this pane
  // and the next; until they click, the config decides, even if it changes.
  const [clicked, setClicked] = useState<string | null>(storedView);
  const view = chooseView(clicked, settings?.officeView);
  const seats = headcount(office.employees.length, settings?.maxConcurrent ?? DEFAULT_SEATS);

  const choose = (next: OfficeViewKind) => {
    rememberView(next);
    setClicked(next);
  };
  const pick = (employee: Employee) => {
    if (employee.agent.pane_id) void focusPane(employee.agent.pane_id).catch(showError);
  };

  return (
    <div className="office">
      {view === "cards" ? (
        <CardsFloor office={office} seats={seats} screens={screens} onPick={pick} />
      ) : (
        <div className="office-empty">The floor map is being built.</div>
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
