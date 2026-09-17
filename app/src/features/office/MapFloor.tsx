import type { Headcount } from "./floor";
import { MapBackdrop } from "./MapBackdrop";
import { MAP_WIDTH, mapHeight } from "./mapGeometry";
import type { Employee, Office } from "./officeStore";
import type { ChatLine } from "./phrasing";
import { Pod, VacantPod } from "./Pod";
import { Walker, useWalks } from "./Walker";

/** Pods in the design's two rows; a map that size fits the pane, a taller one scrolls. */
const FITTING_PODS = 6;

interface Props {
  workspaceId: string;
  office: Office;
  seats: Headcount;
  /** What HR has to say under its button. */
  hrNote: string;
  /** The last few things that happened, as the office would say them. */
  chat: readonly ChatLine[];
  onPick: (employee: Employee) => void;
  onHire: () => void;
}

/**
 * The office as a floor plan. One SVG in the map's own units, scaled to the
 * pane: everything on it — name tags and HR's button included — is inside, so
 * nothing drifts off its desk when the pane is resized.
 */
export function MapFloor({ workspaceId, office, seats, hrNote, chat, onPick, onHire }: Props) {
  // One person crosses the floor at a time; the rest wait their turn at HR.
  const { queue, finish } = useWalks(workspaceId, office.employees);
  const onTheirWay = new Set(queue.filter((walk) => walk.kind === "arrive").map((walk) => walk.id));
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
          employee && onTheirWay.has(employee.agent.id) ? (
            // Theirs already, but they have not reached it yet.
            <VacantPod key={`reserved-${pod}`} pod={pod} sign={`reserved for ${employee.persona.name}`} />
          ) : employee ? (
            <Pod key={employee.agent.id} employee={employee} onPick={onPick} />
          ) : (
            <VacantPod key={`vacant-${pod}`} pod={pod} />
          ),
        )}
        {chat.length > 0 && (
          <foreignObject x="26" y={height - 196} width="360" height="112">
            <div className="office-chat">
              <span className="office-chat-title">Activity</span>
              {chat.map((line) => (
                <span key={line.seq} className={`office-chat-line ${line.tone}`}>
                  <span className="who">{line.who}</span> {line.text}
                </span>
              ))}
            </div>
          </foreignObject>
        )}
        {queue[0] && <Walker key={`${queue[0].kind}-${queue[0].id}`} walk={queue[0]} onDone={finish} />}
      </svg>
    </div>
  );
}
