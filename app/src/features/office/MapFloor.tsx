import type { ReactNode } from "react";
import type { Headcount } from "./floor";
import { MapBackdrop } from "./MapBackdrop";
import { MAP_WIDTH, mapHeight } from "./mapGeometry";
import type { Employee, Office } from "./officeStore";
import { Pod, VacantPod } from "./Pod";

/** Pods in the design's two rows; a map that size fits the pane, a taller one scrolls. */
const FITTING_PODS = 6;

interface Props {
  office: Office;
  seats: Headcount;
  /** What HR has to say under its button. */
  hrNote: string;
  onPick: (employee: Employee) => void;
  onHire: () => void;
  /** Whoever is on their way to or from a desk, drawn above the floor. */
  children?: ReactNode;
}

/**
 * The office as a floor plan. One SVG in the map's own units, scaled to the
 * pane: everything on it — name tags and HR's button included — is inside, so
 * nothing drifts off its desk when the pane is resized.
 */
export function MapFloor({ office, seats, hrNote, onPick, onHire, children }: Props) {
  const height = mapHeight(office.pods.length);
  const fits = office.pods.length <= FITTING_PODS;
  return (
    <div className={`office-map${fits ? " fits" : ""}`}>
      <svg viewBox={`0 0 ${MAP_WIDTH} ${height}`} preserveAspectRatio="xMidYMid meet" role="group" aria-label="Office floor">
        <MapBackdrop height={height} />
        <foreignObject x="44" y="96" width="240" height="30">
          <div className="office-room-label hr">HR office</div>
        </foreignObject>
        <foreignObject x="46" y="228" width="240" height="80">
          <div className="office-map-hr">
            <button type="button" disabled={seats.full} onClick={onHire}>
              {seats.full ? "Office full" : "Hire an agent"}
            </button>
            <span>{hrNote}</span>
          </div>
        </foreignObject>
        <foreignObject x="58" y="544" width="160" height="26">
          <div className="office-room-label break">break room</div>
        </foreignObject>
        {office.pods.map((employee, pod) =>
          employee ? (
            <Pod key={employee.agent.id} employee={employee} onPick={onPick} />
          ) : (
            <VacantPod key={`vacant-${pod}`} pod={pod} />
          ),
        )}
        {children}
      </svg>
    </div>
  );
}
