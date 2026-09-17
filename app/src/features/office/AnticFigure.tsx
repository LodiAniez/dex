import type { AnticKind } from "./antics";
import type { Persona } from "./persona";

interface Props {
  persona: Persona;
  /** Walking somewhere, or the antic being performed. */
  doing: AnticKind | "walking";
  /** Which way they are heading: 1 right, -1 left. A kart and a roll face it. */
  heading: 1 | -1;
}

/** Head, body and legs: the standing figure everything else is drawn around. */
function Body({ persona, legs = true }: { persona: Persona; legs?: boolean }) {
  return (
    <>
      {legs && (
        <>
          <path className="office-leg-a" d="M19 42 L19 56" stroke="#3f4554" strokeWidth="8" strokeLinecap="round" />
          <path className="office-leg-b" d="M29 42 L29 56" stroke="#3f4554" strokeWidth="8" strokeLinecap="round" />
        </>
      )}
      <path d="M12 46 v-12 a12 12 0 0 1 24 0 v12 z" fill={persona.shirt} />
      <circle cx="24" cy="16" r="11" fill={persona.skin} />
      <ellipse cx="24" cy="7" rx="12" ry="6" fill={persona.hair} />
    </>
  );
}

/**
 * An agent out of their cubicle with nothing to do: walking somewhere, or up to
 * one of the things that happen away from a desk. The nap and the song happen
 * at the desk, and the pod draws those.
 */
export function AnticFigure({ persona, doing, heading }: Props) {
  return (
    <>
      <foreignObject x="-36" y="-24" width="120" height="22">
        <div className="office-tag-row">
          <span className="office-tag walker">{persona.name}</span>
        </div>
      </foreignObject>

      {doing === "walking" && (
        <g className="office-walker-body office-strolling">
          <Body persona={persona} />
        </g>
      )}

      {doing === "coffee" && (
        <g className="office-walker-body">
          <Body persona={persona} />
          {/* A mug, lifted to the mouth and lowered again, steaming. */}
          <g className="office-sip">
            <path d="M36 36 L42 30" stroke={persona.skin} strokeWidth="6" strokeLinecap="round" />
            <rect x="39" y="20" width="10" height="11" rx="2.5" fill="#f6f2e6" />
            <path d="M49 23 h2.5 a2.5 2.5 0 0 1 0 5 h-2.5" stroke="#f6f2e6" strokeWidth="2" fill="none" />
            <path className="office-steam" d="M42 17 q3 -4 0 -8 M46 17 q3 -4 0 -8" stroke="#f8f5ec" strokeWidth="1.6" strokeLinecap="round" fill="none" />
          </g>
        </g>
      )}

      {doing === "kart" && (
        // Turned about the middle of the figure, so a kart that turns round stays where it is.
        <g className="office-kart" transform={heading === -1 ? "translate(48 0) scale(-1 1)" : undefined}>
          {/* Low, red, and too small for them. */}
          <rect x="-8" y="40" width="64" height="16" rx="7" fill="#d83a3a" />
          <rect x="40" y="34" width="14" height="10" rx="3" fill="#a92a2a" />
          <rect x="-10" y="36" width="8" height="14" rx="2" fill="#7d1f1f" />
          <circle className="office-wheel" cx="4" cy="58" r="7" fill="#1b1f28" />
          <circle className="office-wheel" cx="44" cy="58" r="7" fill="#1b1f28" />
          <circle cx="4" cy="58" r="2.5" fill="#9aa3b5" />
          <circle cx="44" cy="58" r="2.5" fill="#9aa3b5" />
          <g transform="translate(-2 -6)">
            <Body persona={persona} legs={false} />
            <path d="M34 36 L44 38" stroke={persona.skin} strokeWidth="5" strokeLinecap="round" />
          </g>
          <path className="office-exhaust" d="M-14 48 h-10 M-14 54 h-16" stroke="#c9c2b2" strokeWidth="2.5" strokeLinecap="round" />
        </g>
      )}

      {doing === "rope" && (
        <g>
          <g className="office-jumping">
            <Body persona={persona} />
            <path d="M12 38 L2 34 M36 38 L46 34" stroke={persona.skin} strokeWidth="5" strokeLinecap="round" />
          </g>
          {/* The rope swings over their head and under their feet. */}
          <ellipse className="office-rope" cx="24" cy="30" rx="26" ry="34" fill="none" stroke="#f0bf74" strokeWidth="2.5" />
        </g>
      )}

      {(doing === "roll" || doing === "tumble") && (
        <g className={doing === "roll" ? "office-rolling" : "office-tumbling"} style={{ "--turn": `${heading * 360}deg` } as React.CSSProperties}>
          <Body persona={persona} />
        </g>
      )}
    </>
  );
}
