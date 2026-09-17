import { useEffect, useRef, useState } from "react";
import { canBreakOffAt, pickAntic, planAntic, seedOf, wayHome, type AnticKind } from "./antics";
import { AnticFigure } from "./AnticFigure";
import type { Employee } from "./officeStore";
import { between, deskOf, type Leg, type Spot } from "./walks";
import { noteHome, noteOut } from "./whereabouts";

/** What an idle agent is up to, as the map needs to know it. */
export interface Loafing {
  /** Out of their cubicle: the chair is shown empty. */
  away: boolean;
  /** Something done at the desk, which the pod itself draws. */
  atDesk: "nap" | "sing" | null;
}

/** How long someone sits idle before getting up to anything, and between one thing and the next. */
const BEFORE_SECONDS = 4;
const REST_SECONDS = 5;
/** How often someone in the middle of something checks whether there is work. */
const CHECK_SECONDS = 0.4;

/** Who is standing where in the break room, so that the next one in stands somewhere else. */
const drinkers = new Map<string, { x: number; y: number }>();

interface Props {
  employee: Employee;
  /** The map's height: a kart goes round the block only where there is floor to do it on. */
  mapHeight: number;
  /** Whether they have nothing to do. The moment this goes false they head back to their desk. */
  loafing: boolean;
  onChange: (agentId: string, state: Loafing) => void;
}

/**
 * One agent's idle life. While they have nothing to do they pick something
 * (`pickAntic`), walk to where it happens, do it, walk home, rest, and pick
 * again. Given work at any point they stop and walk back like a normal person;
 * only once they are at their desk does the pod show them working.
 */
export function IdleActor({ employee, mapHeight, loafing, onChange }: Props) {
  const { agent, pod } = employee;
  const idle = useRef(loafing);
  idle.current = loafing;
  const report = useRef(onChange);
  report.current = onChange;
  const since = useRef(agent.status_at);
  since.current = agent.status_at;
  const height = useRef(mapHeight);
  height.current = mapHeight;

  const [at, setAt] = useState({ x: deskOf(pod).x, y: deskOf(pod).y, seconds: 0, steady: false });
  const [doing, setDoing] = useState<AnticKind | "walking" | null>(null);
  const [heading, setHeading] = useState<1 | -1>(1);

  useEffect(() => {
    let cancelled = false;
    // Only the timers still running: this loop lives as long as the agent does.
    const timers = new Set<ReturnType<typeof setTimeout>>();
    const wait = (seconds: number) =>
      new Promise<void>((resolve) => {
        const timer = setTimeout(() => {
          timers.delete(timer);
          resolve();
        }, seconds * 1000);
        timers.add(timer);
      });
    /** Waits up to `seconds`, giving up early if `until` comes true. Says whether it did. */
    const waitUnless = async (seconds: number, until: () => boolean) => {
      for (let waited = 0; waited < seconds; waited += CHECK_SECONDS) {
        if (cancelled || until()) return true;
        await wait(Math.min(CHECK_SECONDS, seconds - waited));
      }
      return cancelled || until();
    };
    const recalled = () => !idle.current;
    const say = (state: Loafing) => report.current(agent.id, state);

    let here: Spot = deskOf(pod);
    // Whatever an earlier mount left behind: they are at their desk now.
    noteHome(agent.id);
    /**
     * Walks `legs`, stopping at the first stop after being recalled where
     * `mayStop` allows it. Says whether it got to the end.
     */
    const walk = async (legs: Leg[], end: Spot, as: AnticKind | "walking", mayStop: ((at: Spot) => boolean) | null) => {
      let lastY = Number.NaN;
      for (const [i, leg] of legs.entries()) {
        if (cancelled) return false;
        if (mayStop?.(here) && recalled()) return false;
        // Down a gap between cubicles they keep facing the way they were going.
        if (leg.x !== here.x) setHeading(leg.x < here.x ? -1 : 1);
        setAt({ x: leg.x, y: leg.y, seconds: leg.seconds, steady: leg.y === lastY || as !== "walking" });
        setDoing(as);
        lastY = leg.y;
        await wait(leg.seconds);
        here = i === legs.length - 1 ? end : { x: leg.x, y: leg.y, corridor: leg.y };
        noteOut(agent.id, here);
      }
      return !cancelled;
    };

    void (async () => {
      let last: AnticKind | undefined;
      for (let round = 0; !cancelled; round += 1) {
        // Nothing to do, and has been for a moment.
        while (!cancelled && recalled()) await wait(CHECK_SECONDS);
        if (await waitUnless(round === 0 ? BEFORE_SECONDS : REST_SECONDS, recalled)) continue;

        const taken = [...drinkers].filter(([id]) => id !== agent.id).map(([, spot]) => spot);
        const kind = pickAntic(agent.id, since.current, round, last);
        last = kind;
        const plan = planAntic(kind, pod, seedOf(agent.id, since.current, round), { taken, mapHeight: height.current });
        if (plan.to === null) {
          say({ away: false, atDesk: plan.kind === "nap" ? "nap" : "sing" });
          await waitUnless(plan.seconds, recalled);
          say({ away: false, atDesk: null });
          continue;
        }

        if (plan.kind === "coffee") drinkers.set(agent.id, plan.to);
        say({ away: true, atDesk: null });
        await wait(0.05); // a frame to be drawn at the desk before setting off
        const arrived = await walk(between(here, plan.to), plan.to, "walking", () => true);
        if (arrived && !recalled()) {
          if (plan.act.length > 0) await walk(plan.act, plan.to, plan.kind, (at) => canBreakOffAt(plan, at));
          else {
            setDoing(plan.kind);
            await waitUnless(plan.seconds, recalled);
          }
        }
        // Home, at a walk, from wherever that left them - whether they finished
        // or were called back to work.
        drinkers.delete(agent.id);
        if (cancelled) return;
        await walk(wayHome(here, pod), deskOf(pod), "walking", null);
        if (cancelled) return;
        noteHome(agent.id);
        setDoing(null);
        say({ away: false, atDesk: null });
      }
    })();

    return () => {
      cancelled = true;
      // Where they were last seen stays on record: if they are leaving, they walk out from there.
      timers.forEach(clearTimeout);
      drinkers.delete(agent.id);
      report.current(agent.id, { away: false, atDesk: null });
    };
  }, [agent.id, pod]);

  if (doing === null) return null;
  return (
    <g
      className="office-walker office-loafer"
      style={{ transform: `translate(${at.x}px, ${at.y}px)`, transition: `transform ${at.seconds}s ${at.steady ? "linear" : "ease-in-out"}` }}
      aria-hidden="true"
    >
      <AnticFigure persona={employee.persona} doing={doing} heading={heading} />
    </g>
  );
}
