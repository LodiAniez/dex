import { useEffect, useRef, useState } from "react";
import { useAgents } from "../agents";
import type { Employee } from "./officeStore";
import type { Persona } from "./persona";
import { DOOR_PAUSE, enqueue, movements, routeOf, type Walk } from "./walks";

/** A walk, and who is doing it — kept with the walk because a leaver is no longer on the staff. */
export interface StaffWalk extends Walk {
  persona: Persona;
}

/** How long a leaver takes to fade at HR's door. */
const FADE_SECONDS = 0.4;

function prefersStill(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

/**
 * The queue of people crossing the floor, fed by changes to the agent list.
 * Lives with the map, so nobody is kept waiting by a view that cannot show them:
 * the cards view seats a new hire at once.
 */
export function useWalks(workspaceId: string, employees: readonly Employee[]): { queue: StaffWalk[]; finish: () => void } {
  const list = useAgents();
  const [queue, setQueue] = useState<StaffWalk[]>([]);
  const before = useRef<NonNullable<typeof list>["agents"] | null>(null);
  // Everyone seen at a desk, so that someone who has just left can still be
  // drawn walking out of the right pod in their own clothes.
  const seen = useRef(new Map<string, { pod: number; persona: Persona }>());

  useEffect(() => {
    if (!list) return;
    const moves = movements(before.current, list.agents, workspaceId);
    before.current = list.agents;
    for (const { agent, pod, persona } of employees) seen.current.set(agent.id, { pod, persona });
    const walks = moves.flatMap((move): StaffWalk[] => {
      const known = seen.current.get(move.id);
      if (move.kind === "leave") seen.current.delete(move.id);
      return known ? [{ ...move, ...known }] : [];
    });
    if (walks.length > 0) setQueue((current) => enqueue(current, walks, { still: prefersStill() }));
  }, [list, employees, workspaceId]);

  return { queue, finish: () => setQueue((current) => current.slice(1)) };
}

/**
 * One person crossing the floor. Steps through the route on timers and lets
 * CSS move them between stops, then says it is done.
 */
export function Walker({ walk, onDone }: { walk: StaffWalk; onDone: () => void }) {
  const { from, legs } = routeOf(walk);
  // -1 is standing at the start; n is on the way to (or at) the end of leg n.
  const [step, setStep] = useState(-1);
  const [fading, setFading] = useState(false);
  const done = useRef(onDone);
  done.current = onDone;

  useEffect(() => {
    setStep(-1);
    setFading(false);
    const timers: ReturnType<typeof setTimeout>[] = [];
    // A new hire pauses at HR's door; a leaver needs only a frame to be drawn where they sat.
    let at = walk.kind === "arrive" ? DOOR_PAUSE : 0.05;
    routeOf(walk).legs.forEach((leg, i) => {
      timers.push(setTimeout(() => setStep(i), at * 1000));
      at += leg.seconds;
    });
    if (walk.kind === "leave") {
      timers.push(setTimeout(() => setFading(true), at * 1000));
      at += FADE_SECONDS;
    }
    timers.push(setTimeout(() => done.current(), at * 1000));
    return () => timers.forEach(clearTimeout);
  }, [walk.id, walk.kind, walk.pod]);

  const at = step < 0 ? from : legs[step];
  const leg = step < 0 ? null : legs[step];
  // Down the side and into the pod ease; the long corridor is a steady walk.
  const easing = leg && leg.x !== (step === 0 ? from.x : legs[step - 1].x) ? "linear" : "ease-in-out";
  const { persona } = walk;
  return (
    <g
      className={`office-walker${step >= 0 && !fading ? " walking" : ""}`}
      style={{
        transform: `translate(${at.x}px, ${at.y}px)`,
        transition: leg ? `transform ${leg.seconds}s ${easing}, opacity ${FADE_SECONDS}s` : `opacity ${FADE_SECONDS}s`,
        opacity: fading ? 0 : 1,
      }}
      aria-hidden="true"
    >
      <foreignObject x="-36" y="-24" width="120" height="22">
        <div className="office-tag-row">
          <span className="office-tag walker">{persona.name}</span>
        </div>
      </foreignObject>
      <g className="office-walker-body">
        <path className="office-leg-a" d="M19 42 L19 56" stroke="#3f4554" strokeWidth="8" strokeLinecap="round" />
        <path className="office-leg-b" d="M29 42 L29 56" stroke="#3f4554" strokeWidth="8" strokeLinecap="round" />
        <path d="M12 46 v-12 a12 12 0 0 1 24 0 v12 z" fill={persona.shirt} />
        {/* A laptop under one arm. */}
        <rect x="34" y="30" width="11" height="12" rx="3" fill="#262b36" />
        <circle cx="24" cy="16" r="11" fill={persona.skin} />
        <ellipse cx="24" cy="7" rx="12" ry="6" fill={persona.hair} />
      </g>
    </g>
  );
}
