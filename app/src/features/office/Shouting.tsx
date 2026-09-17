import { useEffect, useRef, useState } from "react";
import { useAgents } from "../agents";
import { POD, podOrigin } from "./mapGeometry";
import type { Persona } from "./persona";
import { HEAD_START_SECONDS, SHOUT_SECONDS, hear, holdsTheDoor, isShouting, newHires, shoutText, type Shout } from "./shout";

/**
 * Who is shouting to HR for staff right now, read off the agent list: an agent
 * that appears with a parent was sent for by it. `holding` is true for the
 * first moment of a shout, while the hire waits inside HR's door.
 */
export function useShout(workspaceId: string): { shout: Shout | null; holding: boolean } {
  const list = useAgents();
  const [shout, setShout] = useState<Shout | null>(null);
  const [, setTick] = useState(0);
  const known = useRef<Set<string> | null>(null);

  useEffect(() => {
    if (!list) return;
    const here = list.agents.filter((agent) => agent.workspace_id === workspaceId);
    const hires = newHires(known.current, here);
    known.current = new Set(here.map((agent) => agent.id));
    if (hires.length > 0) setShout((current) => hires.reduce<Shout | null>((so, hire) => hear(so, hire.hirer, Date.now()), current));
  }, [list, workspaceId]);

  // Look again when the hire may come out, and when the shout has died away.
  useEffect(() => {
    if (!shout) return;
    const again = () => setTick((tick) => tick + 1);
    const timers = [HEAD_START_SECONDS, SHOUT_SECONDS].map((seconds) => setTimeout(again, Math.max(0, shout.heardAt + seconds * 1000 - Date.now()) + 20));
    return () => timers.forEach(clearTimeout);
  }, [shout]);

  const now = Date.now();
  return { shout: isShouting(shout, now) ? shout : null, holding: holdsTheDoor(shout, now) };
}

/**
 * What is shouted, over the shouter's cubicle, its tail at their mouth. Drawn
 * after the pods so that no neighbour's furniture is on top of it.
 */
export function ShoutBubble({ pod, count }: { pod: number; count: number }) {
  const { x, y } = podOrigin(pod);
  return (
    // Keyed on the count: one more sent for, and it is shouted afresh.
    <g key={count} transform={`translate(${x} ${y})`} aria-hidden="true">
      <g className="office-shout">
        {/* Lines of sound, toward HR. */}
        <path className="office-shout-lines" d="M70 136 l-22 -8 M68 150 h-26 M70 164 l-22 8" stroke="#c23b38" strokeWidth="4" strokeLinecap="round" fill="none" />
        <foreignObject x={-24} y={40} width={POD.width + 48} height="84">
          <div className="office-shout-row">
            <span className="office-shout-text">{shoutText(count)}</span>
          </div>
        </foreignObject>
        <path d="M112 118 l14 18 l8 -18 z" fill="#fff6d6" stroke="#c23b38" strokeWidth="3" strokeLinejoin="round" />
        <path d="M110 117 h26" stroke="#fff6d6" strokeWidth="5" />
      </g>
    </g>
  );
}

/** The shouter in their chair, in the pod's own units: hands cupped to an open mouth, shaking with the effort. */
export function Shouter({ persona }: { persona: Persona }) {
  return (
    <g className="office-shouting">
      <path d="M100 154 v-16 a22 22 0 0 1 44 0 v16 z" fill={persona.shirt} />
      <circle cx="122" cy="118" r="13" fill={persona.skin} />
      <path d="M107 116 a15 15 0 0 1 30 0 v-2 a15 12 0 0 0 -30 0 z" fill={persona.hair} />
      <ellipse cx="122" cy="107" rx="15" ry="8" fill={persona.hair} />
      <ellipse cx="122" cy="124" rx="4.5" ry="5" fill="#5a2a2a" />
      <path d="M102 142 L110 126 M142 142 L134 126" stroke={persona.skin} strokeWidth="7" strokeLinecap="round" />
      <circle cx="110" cy="124" r="5.5" fill={persona.skin} />
      <circle cx="134" cy="124" r="5.5" fill={persona.skin} />
    </g>
  );
}
