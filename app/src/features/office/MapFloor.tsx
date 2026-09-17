import { useCallback, useState } from "react";
import { isLoafing } from "./antics";
import type { Headcount } from "./floor";
import { Horn } from "./Announce";
import { IdleActor, type Loafing } from "./IdleActor";
import { MapBackdrop } from "./MapBackdrop";
import { MAP_WIDTH, mapHeight } from "./mapGeometry";
import type { Employee, Office } from "./officeStore";
import type { ChatLine } from "./phrasing";
import { Pod, VacantPod } from "./Pod";
import { Walker, prefersStill, useWalks } from "./Walker";
import { awayFromDesk, expectingVisitor } from "./walks";

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
  /** The workspace's activity: a message in it becomes a walk to the recipient's desk. */
  events: readonly { seq: number; kind: string; agent_id: string | null; target_agent_id: string | null }[] | undefined;
  onPick: (employee: Employee) => void;
  onHire: () => void;
  onAnnounce: () => void;
}

/**
 * The office as a floor plan. One SVG in the map's own units, scaled to the
 * pane: everything on it — name tags and HR's button included — is inside, so
 * nothing drifts off its desk when the pane is resized.
 */
export function MapFloor({ workspaceId, office, seats, hrNote, chat, events, onPick, onHire, onAnnounce }: Props) {
  // One person crosses the floor at a time; the rest wait their turn at HR.
  const { queue, finish } = useWalks(workspaceId, office.employees, events);
  const away = awayFromDesk(queue);
  const visited = expectingVisitor(queue);
  // Whose desk a message is being delivered at right now: they are shown answering.
  const [talkingAt, setTalkingAt] = useState<number | null>(null);
  const onTheirWay = new Set(queue.filter((walk) => walk.kind === "arrive").map((walk) => walk.id));
  // What each idle agent is up to. They come and go on their own: nobody with
  // nothing to do holds up a hire or a message.
  const [loafing, setLoafing] = useState<ReadonlyMap<string, Loafing>>(new Map());
  const onLoaf = useCallback((agentId: string, state: Loafing) => {
    setLoafing((before) => {
      const was = before.get(agentId);
      if ((was?.away ?? false) === state.away && (was?.atDesk ?? null) === state.atDesk) return before;
      const next = new Map(before);
      if (state.away || state.atDesk) next.set(agentId, state);
      else next.delete(agentId);
      return next;
    });
  }, []);
  const still = prefersStill();
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
            <Pod
              key={employee.agent.id}
              employee={employee} away={away === employee.agent.id || loafing.get(employee.agent.id)?.away === true}
              antic={loafing.get(employee.agent.id)?.atDesk ?? null}
             
              listening={talkingAt === pod}
              onPick={onPick}
            />
          ) : (
            <VacantPod key={`vacant-${pod}`} pod={pod} />
          ),
        )}
        {/* The public address: middle right, in the corridor beside the water cooler.
            Walks turn off the corridor before here, and it is drawn after the pods. */}
        <foreignObject x="1100" y="364" width="98" height="112">
          <button type="button" className="office-map-announce" disabled={office.employees.length === 0} onClick={onAnnounce} title="Prompt every agent at once">
            <Horn size={56} />
            <span>Announce</span>
          </button>
        </foreignObject>
        {chat.length > 0 && (
          // Under the break room, not over it: people drink their coffee in there.
          <foreignObject x="26" y={height - 166} width="360" height="112">
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
        {!still &&
          office.employees
            .filter((employee) => !onTheirWay.has(employee.agent.id))
            .map((employee) => (
              <IdleActor
                key={`idle-${employee.agent.id}`}
                employee={employee}
                // Someone out with a message, or about to be brought one, is not idling.
                loafing={isLoafing(employee.agent) && away !== employee.agent.id && !visited.includes(employee.pod)}
                onChange={onLoaf}
              />
            ))}
        {/* Whoever is next across the floor sets off from their desk: if they are out fooling around, once they are back at it. */}
        {queue[0] && loafing.get(queue[0].id)?.away !== true && <Walker key={queue[0].key ?? `${queue[0].kind}-${queue[0].id}`} walk={queue[0]} onTalk={setTalkingAt} onDone={finish} />}
      </svg>
    </div>
  );
}
