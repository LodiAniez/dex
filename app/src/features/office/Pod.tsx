import type { KeyboardEvent } from "react";
import { STATUS_WORDS, askedYou, lastSaid, seenAs } from "../agents";
import { poseOf, type Pose } from "./floor";
import { POD, podOrigin } from "./mapGeometry";
import type { Employee } from "./officeStore";
import { Napper, Singer } from "./PodAntics";
import { Shouter } from "./Shouting";
import { TalkBubble } from "./Walker";

/** The two lines of "code" on a desk screen: their colours say what the agent is doing. */
const SCREEN: Record<Pose, [string, string]> = {
  typing: ["#7fd1c0", "#f0bf74"],
  raised: ["#4f8cff", "#4f8cff"],
  error: ["#f25f5c", "#f25f5c"],
  still: ["#3f4554", "#3f4554"],
};

/** Matches `.agent-dot` in the shell's styles: one status, one colour, everywhere. */
const DOT: Record<string, string> = {
  idle: "#8a93a6",
  running: "#f5a524",
  waiting: "#4f8cff",
  error: "#f25f5c",
  unknown: "#ff8c42",
};

/** Desk, monitor and chair: the same in an occupied pod and an empty one. */
function Furniture({ plant }: { plant: boolean }) {
  return (
    <>
      <rect x="42" y="56" width="160" height="40" rx="10" fill="#96613a" />
      <rect x="42" y="48" width="160" height="14" rx="7" fill="#b07a48" />
      <rect x="50" y="96" width="10" height="16" rx="4" fill="#77492a" />
      <rect x="184" y="96" width="10" height="16" rx="4" fill="#77492a" />
      <rect x="94" y="26" width="56" height="32" rx="6" fill="#262b36" />
      <rect x="99" y="31" width="46" height="19" rx="4" fill="#131822" />
      <rect x="116" y="58" width="12" height="5" rx="2" fill="#1b1f28" />
      {plant && (
        <>
          <circle cx="215" cy="66" r="12" fill="#4caf6d" />
          <circle cx="222" cy="56" r="8" fill="#5fc27f" />
          <rect x="206" y="76" width="18" height="16" rx="4" fill="#b5563a" />
        </>
      )}
      <rect x="94" y="164" width="56" height="16" rx="7" fill="#3f4554" />
      <rect x="100" y="146" width="44" height="22" rx="9" fill="#4a5162" />
    </>
  );
}

/** A name tag or a "vacant" sign. HTML, so the pill fits whatever it says. */
function Tag({ children }: { children: React.ReactNode }) {
  return (
    <foreignObject x="0" y="0" width={POD.width} height="28">
      <div className="office-tag-row">{children}</div>
    </foreignObject>
  );
}

interface PodProps {
  employee: Employee;
  /** Out delivering a message: the desk is theirs, the chair is empty. */
  away?: boolean;
  /** A colleague is at their desk with a message: they are shown answering. */
  listening?: boolean;
  /** Nothing to do, and passing the time at their desk. Work, or a visitor, comes first. */
  antic?: "nap" | "sing" | null;
  /** Sending to HR for staff, at the top of their voice. */
  shouting?: boolean;
  onPick: (employee: Employee) => void;
}

export function Pod({ employee, away = false, listening = false, antic = null, shouting = false, onPick }: PodProps) {
  const { agent, persona, role, pod } = employee;
  const { x, y } = podOrigin(pod);
  // As the owner should see them: someone who asked them something has a hand up.
  const shown = seenAs(agent);
  const pose = poseOf(shown);
  // What they asked the owner goes in a speech bubble at their mouth; what they merely said, quietly under the desk.
  const asked = away || shouting ? null : askedYou(agent);
  const said = askedYou(agent) === null ? lastSaid(agent) : null;
  const typing = pose === "typing";
  const passing = !away && !listening && !shouting && pose === "still" ? antic : null;
  const [code1, code2] = SCREEN[pose];
  // Line lengths differ from desk to desk so six screens do not tap in unison.
  const line1 = 22 + ((pod * 7) % 16);
  const line2 = 12 + ((pod * 11) % 22);
  const onKey = (event: KeyboardEvent) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    onPick(employee);
  };
  return (
    <g
      className="office-pod"
      transform={`translate(${x} ${y})`}
      role="button"
      tabIndex={0}
      aria-label={`${persona.name}, ${role}, ${STATUS_WORDS[shown]}`}
      onClick={() => onPick(employee)}
      onKeyDown={onKey}
    >
      {/* Something to hit between the furniture. */}
      <rect width={POD.width} height={POD.height} fill="transparent" />
      <Tag>
        <span className="office-tag">
          <span className="office-tag-dot" style={{ background: DOT[shown] ?? DOT.idle }} />
          {persona.name}
          <span className="office-tag-role">{role}</span>
        </span>
      </Tag>
      {/* How their turn ended, so nobody has to open a pane to see it. */}
      {said !== null && (
        <foreignObject x="6" y={POD.height - 6} width={POD.width - 12} height="24">
          <div className="office-said" title={said}>
            “{said}”
          </div>
        </foreignObject>
      )}
      <g transform="translate(4 30)">
        <rect x="6" y="26" width="232" height="166" rx="18" fill={persona.rug} />
        <rect x="6" y="26" width="232" height="166" rx="18" fill="none" stroke={persona.rugLine} strokeWidth="4" />
        <Furniture plant />
        <rect className={typing ? "office-tap1" : undefined} x="103" y="36" width={line1} height="4" rx="2" fill={code1} />
        <rect className={typing ? "office-tap2" : undefined} x="103" y="43" width={line2} height="4" rx="2" fill={code2} />
        {passing === "nap" && <Napper persona={persona} />}
        {passing === "sing" && <Singer persona={persona} />}
        {!away && shouting && <Shouter persona={persona} />}
        {!away && !shouting && passing === null && (
          <>
            {pose === "raised" && (
              <path d="M186 128 Q192 112 188 102" stroke={persona.skin} strokeWidth="8" strokeLinecap="round" fill="none" />
            )}
            <path d="M100 154 v-16 a22 22 0 0 1 44 0 v16 z" fill={persona.shirt} />
            <circle className={typing ? "office-tap1" : undefined} cx="96" cy="138" r="5.5" fill={persona.skin} />
            <circle className={typing ? "office-tap2" : undefined} cx="148" cy="138" r="5.5" fill={persona.skin} />
            <circle cx="122" cy="118" r="13" fill={persona.skin} />
            <path d="M107 116 a15 15 0 0 1 30 0 v-2 a15 12 0 0 0 -30 0 z" fill={persona.hair} />
            <ellipse cx="122" cy="107" rx="15" ry="8" fill={persona.hair} />
            {listening && <TalkBubble x={150} y={84} turn="b" />}
            {!listening && asked === null && (pose === "raised" || pose === "error") && (
              <g className="office-bubble">
                {/* Rounded on three corners, pointed at the one nearest the speaker. */}
                <path d="M194 4 h8 a10 10 0 0 1 10 10 v6 a10 10 0 0 1 -10 10 h-16 a2 2 0 0 1 -2 -2 v-14 a10 10 0 0 1 10 -10 z" fill="#f8f5ec" />
                <text x="198" y="23" textAnchor="middle" fontSize="16" fontWeight="600" fill={pose === "error" ? "#c23b38" : "#185fa5"}>
                  {pose === "error" ? "!" : "?"}
                </text>
              </g>
            )}
          </>
        )}
      </g>
      {/* Last, so it is over their own desk: they are saying it, and waiting to hear. */}
      {asked !== null && !listening && <AskBubble question={asked} />}
    </g>
  );
}

/** What an agent asked the owner, in a speech bubble with its tail at their mouth. In the pod's own units. */
function AskBubble({ question }: { question: string }) {
  return (
    <g className="office-ask" aria-hidden="true">
      <foreignObject x="8" y="24" width={POD.width - 16} height="104">
        <div className="office-ask-row">
          <span className="office-ask-text" title={question}>
            {question}
          </span>
        </div>
      </foreignObject>
      {/* The tail, from the bubble's foot to just over their head; the second path hides the border it crosses. */}
      <path d="M116 126 l12 17 l10 -17 z" fill="#f8f5ec" stroke="#185fa5" strokeWidth="2.5" strokeLinejoin="round" />
      <path d="M115 125.5 h24" stroke="#f8f5ec" strokeWidth="4" />
    </g>
  );
}

export function VacantPod({ pod, sign }: { pod: number; sign?: string }) {
  const { x, y } = podOrigin(pod);
  return (
    <g transform={`translate(${x} ${y})`} aria-hidden="true">
      <Tag>
        <span className="office-tag vacant">{sign ?? `office ${pod + 1} — vacant`}</span>
      </Tag>
      <g transform="translate(4 30)" opacity="0.8">
        <rect x="6" y="26" width="232" height="166" rx="18" fill="#d3ccba" />
        <rect x="6" y="26" width="232" height="166" rx="18" fill="none" stroke="#b3ab97" strokeWidth="4" />
        <Furniture plant={false} />
      </g>
    </g>
  );
}
