import type { Persona } from "./persona";

/**
 * The two antics that happen in the cubicle, drawn in the pod's own units
 * (inside its rug). The rest happen out on the floor: see `AnticFigure.tsx`.
 */

/** Asleep on a camp bed pulled out in front of the chair. */
export function Napper({ persona }: { persona: Persona }) {
  return (
    <g>
      <rect x="44" y="134" width="164" height="46" rx="10" fill="#6b4a2f" />
      <rect x="48" y="130" width="156" height="42" rx="9" fill="#f1ece0" />
      <rect x="52" y="134" width="40" height="34" rx="9" fill="#ffffff" />
      <circle cx="74" cy="151" r="13" fill={persona.skin} />
      <path d="M61 149 a13 13 0 0 1 26 -2 a15 10 0 0 0 -26 2 z" fill={persona.hair} />
      <ellipse cx="72" cy="140" rx="14" ry="6" fill={persona.hair} />
      {/* Eyes shut. */}
      <path d="M68 153 q3 2.5 6 0 M77 153 q3 2.5 6 0" stroke="#3a2f2a" strokeWidth="1.6" strokeLinecap="round" fill="none" />
      <path className="office-blanket" d="M90 172 v-30 a10 10 0 0 1 10 -10 h94 a10 10 0 0 1 10 10 v30 z" fill={persona.shirt} />
      <path d="M90 146 h114" stroke="#ffffff" strokeOpacity="0.35" strokeWidth="5" />
      {(["", " second", " third"] as const).map((delay, i) => (
        <text key={delay} className={`office-snore${delay}`} x={92 + i * 10} y={126 - i * 8} fontSize={12 + i * 3} fontWeight="700" fill="#185fa5">
          z
        </text>
      ))}
    </g>
  );
}

/** In their chair, swaying, mouth open, notes going up. */
export function Singer({ persona }: { persona: Persona }) {
  return (
    <g>
      <g className="office-singing">
        <path d="M100 154 v-16 a22 22 0 0 1 44 0 v16 z" fill={persona.shirt} />
        {/* A fist for a microphone. */}
        <path d="M144 146 L136 132" stroke={persona.skin} strokeWidth="7" strokeLinecap="round" />
        <rect x="131" y="120" width="7" height="12" rx="3.5" fill="#1b1f28" />
        <circle cx="122" cy="118" r="13" fill={persona.skin} />
        <path d="M107 116 a15 15 0 0 1 30 0 v-2 a15 12 0 0 0 -30 0 z" fill={persona.hair} />
        <ellipse cx="122" cy="107" rx="15" ry="8" fill={persona.hair} />
        <ellipse cx="122" cy="124" rx="3.2" ry="4" fill="#5a2a2a" />
      </g>
      {(["", " second", " third"] as const).map((delay, i) => (
        <text key={delay} className={`office-note${delay}`} x={156 + i * 20} y={150 - (i % 2) * 12} fontSize="24" fontWeight="700" fill={["#7f3fbf", "#d83a3a", "#185fa5"][i]}>
          {i === 1 ? "♫" : "♪"}
        </text>
      ))}
    </g>
  );
}
