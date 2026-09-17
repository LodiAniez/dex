import { useEffect, useRef, useState } from "react";
import { useAgents } from "../agents";
import type { Employee } from "./officeStore";
import type { Persona } from "./persona";
import { takeWhereabouts } from "./whereabouts";
import { DOOR_PAUSE, TALK_SECONDS, WAVE_SECONDS, afterWalk, deliveries, deskOf, enqueue, movements, nextLegs, routeOf, type Walk } from "./walks";

/** A walk, and who is doing it — kept with the walk because a leaver is no longer on the staff. */
export interface StaffWalk extends Walk {
  persona: Persona;
}

/** How long a leaver takes to fade at HR's door. */
const FADE_SECONDS = 0.4;

export function prefersStill(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

/**
 * The queue of people crossing the floor, fed by changes to the agent list.
 * Lives with the map, so nobody is kept waiting by a view that cannot show them:
 * the cards view seats a new hire at once.
 */
/** The part of an activity event a delivery is read from. */
interface LoggedEvent {
  seq: number;
  kind: string;
  agent_id: string | null;
  target_agent_id: string | null;
}

export function useWalks(
  workspaceId: string,
  employees: readonly Employee[],
  events: readonly LoggedEvent[] | undefined,
): { queue: StaffWalk[]; finish: (visited?: number) => void } {
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
      if (move.kind !== "leave") return known ? [{ ...move, ...known }] : [];
      seen.current.delete(move.id);
      // Out fooling around when they ended: they leave from there, not from a desk they were not at.
      const from = takeWhereabouts(move.id) ?? undefined;
      return known ? [{ ...move, ...known, from }] : [];
    });
    if (walks.length > 0) setQueue((current) => enqueue(current, walks, { still: prefersStill() }));
  }, [list, employees, workspaceId]);

  // Messages become trips: the sender walks to the recipient's desk and back.
  // Whatever was already in the log when the map opened happened before anyone
  // was watching, so only what comes after it walks.
  const sinceSeq = useRef<number | null>(null);
  useEffect(() => {
    if (!events) return;
    const newest = events.reduce((max, event) => Math.max(max, event.seq), 0);
    if (sinceSeq.current === null) {
      sinceSeq.current = newest;
      return;
    }
    const seats = new Map(employees.map(({ agent, pod }) => [agent.id, pod]));
    const trips = deliveries(events, sinceSeq.current, seats).flatMap((walk): StaffWalk[] => {
      const sender = employees.find(({ agent }) => agent.id === walk.id);
      return sender ? [{ ...walk, persona: sender.persona }] : [];
    });
    sinceSeq.current = newest;
    if (trips.length > 0) setQueue((current) => enqueue(current, trips, { still: prefersStill() }));
  }, [events, employees]);

  return { queue, finish: (visited) => setQueue((current) => afterWalk(current, visited)) };
}

interface WalkerProps {
  walk: StaffWalk;
  /** Whose desk the walker is talking at, or null; the map shows them answering. */
  onTalk: (pod: number | null) => void;
  /** Finished; a round says how many stops it made, in case it grew as it ended. */
  onDone: (visited?: number) => void;
}

/** Whoever is crossing the floor: a hire or a leaver on a fixed route, or someone on a round of messages. */
export function Walker(props: WalkerProps) {
  return props.walk.kind === "deliver" ? <RoundWalker {...props} /> : <RouteWalker walk={props.walk} onDone={() => props.onDone()} />;
}

/** The figure itself, wherever it is and whatever it is doing. */
function Figure({ walk, talking, waving = false }: { walk: StaffWalk; talking: boolean; waving?: boolean }) {
  const { persona } = walk;
  return (
    <>
      <foreignObject x="-36" y="-24" width="120" height="22">
        <div className="office-tag-row">
          <span className="office-tag walker">{persona.name}</span>
        </div>
      </foreignObject>
      {talking && <TalkBubble x={40} y={-30} turn="a" />}
      <g className={`office-walker-body${talking ? " talking" : ""}`}>
        <path className="office-leg-a" d="M19 42 L19 56" stroke="#3f4554" strokeWidth="8" strokeLinecap="round" />
        <path className="office-leg-b" d="M29 42 L29 56" stroke="#3f4554" strokeWidth="8" strokeLinecap="round" />
        {/* Goodbye: an arm up over the shoulder, waving from the elbow. */}
        {waving && <path className="office-wave" d="M35 32 L43 17" stroke={persona.skin} strokeWidth="6" strokeLinecap="round" fill="none" />}
        <path d="M12 46 v-12 a12 12 0 0 1 24 0 v12 z" fill={persona.shirt} />
        {/* Under one arm: a laptop, or the message being carried. */}
        {!waving && <rect x="34" y="30" width="11" height="12" rx="3" fill={walk.kind === "deliver" ? "#f8f5ec" : "#262b36"} />}
        <circle cx="24" cy="16" r="11" fill={persona.skin} />
        <ellipse cx="24" cy="7" rx="12" ry="6" fill={persona.hair} />
      </g>
    </>
  );
}

/**
 * A speech bubble with three dots that take turns. Two people talking get one
 * each, on opposite turns ("a" speaks first), so it reads as an exchange.
 */
export function TalkBubble({ x, y, turn }: { x: number; y: number; turn: "a" | "b" }) {
  return (
    <g className={`office-talk turn-${turn}`} transform={`translate(${x} ${y})`} aria-hidden="true">
      <path d="M10 0 h20 a10 10 0 0 1 10 10 v6 a10 10 0 0 1 -10 10 h-28 a2 2 0 0 1 -2 -2 v-14 a10 10 0 0 1 10 -10 z" fill="#f8f5ec" />
      {[11, 20, 29].map((cx, i) => (
        <circle key={cx} className={`office-talk-dot d${i}`} cx={cx} cy="13" r="2.6" fill="#3f4554" />
      ))}
    </g>
  );
}

/**
 * Someone on a round of messages. The round is decided a step at a time
 * (`nextLegs`), because it can grow while they are out: from one colleague
 * straight to the next, a conversation at each desk, and home at the end.
 */
function RoundWalker({ walk, onTalk, onDone }: WalkerProps) {
  const latest = useRef(walk);
  latest.current = walk;
  const calls = useRef({ onTalk, onDone });
  calls.current = { onTalk, onDone };
  const [at, setAt] = useState(() => ({ x: deskOf(walk.pod).x, y: deskOf(walk.pod).y, seconds: 0, steady: false }));
  const [moving, setMoving] = useState(false);
  const [talking, setTalking] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const timers: ReturnType<typeof setTimeout>[] = [];
    const wait = (seconds: number) => new Promise<void>((resolve) => timers.push(setTimeout(resolve, seconds * 1000)));
    void (async () => {
      let visited = 0;
      let where: number | "home" = "home";
      // A frame to be drawn at their desk before setting off.
      await wait(0.05);
      for (;;) {
        const next = nextLegs(latest.current, visited, where);
        if (cancelled || !next) break;
        let lastY = Number.NaN;
        for (const leg of next.legs) {
          if (cancelled) return;
          // The corridor is a steady walk; turning in and out of a pod eases.
          setAt({ x: leg.x, y: leg.y, seconds: leg.seconds, steady: leg.y === lastY });
          setMoving(true);
          lastY = leg.y;
          await wait(leg.seconds);
        }
        if (cancelled) return;
        setMoving(false);
        where = next.arrives;
        if (where !== "home") {
          setTalking(true);
          calls.current.onTalk(where);
          await wait(TALK_SECONDS);
          if (cancelled) return;
          setTalking(false);
          calls.current.onTalk(null);
          visited += 1;
        }
      }
      if (!cancelled) calls.current.onDone(visited);
    })();
    return () => {
      cancelled = true;
      timers.forEach(clearTimeout);
      calls.current.onTalk(null);
    };
  }, [walk.key, walk.id]);

  return (
    <g
      className={`office-walker${moving ? " walking" : ""}`}
      style={{ transform: `translate(${at.x}px, ${at.y}px)`, transition: `transform ${at.seconds}s ${at.steady ? "linear" : "ease-in-out"}` }}
      aria-hidden="true"
    >
      <Figure walk={walk} talking={talking} />
    </g>
  );
}

/**
 * A hire or a leaver crossing the floor on a fixed route. Steps through it on
 * timers and lets CSS move them between stops, then says it is done.
 */
function RouteWalker({ walk, onDone }: { walk: StaffWalk; onDone: () => void }) {
  const { from, legs } = routeOf(walk);
  // -1 is standing at the start; n is on the way to (or at) the end of leg n.
  const [step, setStep] = useState(-1);
  const [fading, setFading] = useState(false);
  // At the door, before going: they turn and wave.
  const [waving, setWaving] = useState(false);
  const done = useRef(onDone);
  done.current = onDone;

  useEffect(() => {
    setStep(-1);
    setFading(false);
    setWaving(false);
    const timers: ReturnType<typeof setTimeout>[] = [];
    // A new hire pauses at HR's door; anyone leaving a desk needs only a frame to be drawn where they sat.
    let at = walk.kind === "arrive" ? DOOR_PAUSE : 0.05;
    routeOf(walk).legs.forEach((leg, i) => {
      timers.push(setTimeout(() => setStep(i), at * 1000));
      at += leg.seconds;
    });
    if (walk.kind === "leave") {
      timers.push(setTimeout(() => setWaving(true), at * 1000));
      at += WAVE_SECONDS;
      timers.push(setTimeout(() => setFading(true), at * 1000));
      at += FADE_SECONDS;
    }
    timers.push(setTimeout(() => done.current(), at * 1000));
    return () => timers.forEach(clearTimeout);
  }, [walk.id, walk.kind, walk.pod, walk.from]);

  const at = step < 0 ? from : legs[step];
  const leg = step < 0 ? null : legs[step];
  const before = step <= 0 ? from : legs[step - 1];
  // Down the side and into the pod ease; the long corridor is a steady walk.
  const easing = leg && leg.x !== before.x ? "linear" : "ease-in-out";
  // Standing at a colleague's desk handing something over is not walking.
  const moving = leg !== null && (leg.x !== before.x || leg.y !== before.y);
  return (
    <g
      className={`office-walker${moving && !waving && !fading ? " walking" : ""}`}
      style={{
        transform: `translate(${at.x}px, ${at.y}px)`,
        transition: leg ? `transform ${leg.seconds}s ${easing}, opacity ${FADE_SECONDS}s` : `opacity ${FADE_SECONDS}s`,
        opacity: fading ? 0 : 1,
      }}
      aria-hidden="true"
    >
      <Figure walk={walk} talking={false} waving={waving} />
    </g>
  );
}
