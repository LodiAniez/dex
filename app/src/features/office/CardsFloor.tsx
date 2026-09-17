import { AgentCard } from "./AgentCard";
import type { Headcount } from "./floor";
import type { Employee, Office } from "./officeStore";

interface Props {
  office: Office;
  seats: Headcount;
  /** What HR has to say under its button. */
  hrNote: string;
  /** The last lines on each agent's screen, by agent id; a card shows the last `cardLines`. */
  screens: ReadonlyMap<string, string[]>;
  cardLines: number;
  onGoToPane: (paneId: string) => void;
  /** Opens someone's panel, from the Details button on their card. */
  onDetails: (employee: Employee) => void;
  onHire: () => void;
}

/** The office as a grid of cards, one per pod, with HR's column beside it. */
export function CardsFloor({ office, seats, hrNote, screens, cardLines, onGoToPane, onDetails, onHire }: Props) {
  return (
    <div className="office-cards">
      <aside className="office-hr">
        <div className="office-hr-title">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
            <rect x="5" y="3" width="14" height="18" rx="2" />
            <circle cx="12" cy="9" r="2.4" />
            <path d="M8.5 16.5c.7-1.8 2-2.7 3.5-2.7s2.8.9 3.5 2.7" />
          </svg>
          HR office
        </div>
        <div className={`office-seats${seats.full ? " full" : ""}`}>{seats.text}</div>
        <button type="button" className="office-hire-open" disabled={seats.full} onClick={onHire}>
          {seats.full ? "Office full" : "Hire an agent"}
        </button>
        <div className="office-hr-note">{hrNote}</div>
        <div className="office-hr-hint">Each card is that agent's screen. Type under it to prompt them.</div>
      </aside>
      <div className="office-grid">
        {office.pods.map((employee, pod) =>
          employee ? (
            <AgentCard
              key={employee.agent.id}
              employee={employee}
              screen={(screens.get(employee.agent.id) ?? []).slice(-cardLines)}
              onGoToPane={onGoToPane}
              onDetails={onDetails}
            />
          ) : (
            // Keyed by pod: a vacancy is a place, not a person.
            <div key={`vacant-${pod}`} className="office-vacant">
              <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
                <path d="M4 20v-9l8-6 8 6v9" />
                <path d="M10 20v-5h4v5" />
              </svg>
              <span>office {pod + 1} — vacant</span>
            </div>
          ),
        )}
      </div>
    </div>
  );
}
