import { MAP_WIDTH, WALL_INSET, floorLines } from "./mapGeometry";

const PLANT = { leaf: "#4caf6d", leafLight: "#5fc27f", pot: "#b5563a" } as const;
const WOOD = { top: "#b07a48", side: "#96613a" } as const;

/**
 * The building nobody clicks: floor, wall, windows, HR's room, the break room
 * and the plants. Drawn for a map `height` tall; what belongs at the bottom
 * moves down with it.
 */
export function MapBackdrop({ height }: { height: number }) {
  const inner = MAP_WIDTH - WALL_INSET * 2;
  const floorBottom = height - WALL_INSET;
  return (
    <g aria-hidden="true">
      <rect x="0" y="0" width={MAP_WIDTH} height={height} fill="#20242e" />
      <rect x={WALL_INSET} y="24" width={inner} height={height - 38} rx="26" fill="#e4b57e" />
      <rect x={WALL_INSET} y="24" width={inner} height="44" rx="22" fill="#5a6172" />
      <rect x={WALL_INSET} y="52" width={inner} height="16" fill="#4a5060" />
      {[120, 480, 840, 1150].map((x) => (
        <rect key={x} x={x} y="32" width="64" height="28" rx="8" fill="#bfe6f4" />
      ))}
      <g opacity="0.35" stroke="#c99a5e" strokeWidth="2">
        {floorLines(height).map((y) => (
          <line key={`h${y}`} x1={WALL_INSET} y1={y} x2={MAP_WIDTH - WALL_INSET} y2={y} />
        ))}
        {[320, 640, 960].map((x) => (
          <line key={`v${x}`} x1={x} y1="68" x2={x} y2={floorBottom} />
        ))}
      </g>
      <HrRoom />
      <BreakRoom />
      {/* Water cooler, on the right-hand wall. */}
      <rect x="1206" y="380" width="40" height="62" rx="9" fill="#d5dbe6" />
      <rect x="1214" y="392" width="24" height="26" rx="6" fill="#bfe6f4" />
      <circle cx="1226" cy="452" r="9" fill="#9aa3b5" />
      {/* Two plants that stay in the bottom corners however tall the building. */}
      <circle cx="1212" cy={height - 90} r="16" fill={PLANT.leaf} />
      <circle cx="1226" cy={height - 104} r="12" fill={PLANT.leafLight} />
      <rect x="1202" y={height - 82} width="26" height="24" rx="6" fill={PLANT.pot} />
      <circle cx="346" cy={height - 76} r="15" fill={PLANT.leaf} />
      <rect x="336" y={height - 68} width="22" height="22" rx="6" fill={PLANT.pot} />
    </g>
  );
}

function HrRoom() {
  return (
    <g>
      <rect x="30" y="84" width="272" height="258" rx="20" fill="#a9d8cd" />
      <rect x="30" y="84" width="272" height="258" rx="20" fill="none" stroke="#7fb8aa" strokeWidth="5" />
      <rect x="62" y="176" width="180" height="38" rx="10" fill={WOOD.side} />
      <rect x="62" y="168" width="180" height="14" rx="7" fill={WOOD.top} />
      <rect x="134" y="142" width="40" height="28" rx="5" fill="#262b36" />
      <rect x="139" y="147" width="30" height="16" rx="3" fill="#131822" />
      <rect x="142" y="151" width="16" height="3" rx="1.5" fill="#7fd1c0" />
      <rect x="142" y="156" width="10" height="3" rx="1.5" fill="#f0bf74" />
      <circle cx="152" cy="112" r="11" fill="#8d5a3b" />
      <path d="M140 138 a12 12 0 0 1 24 0 z" fill="#5f6d8c" />
      <circle cx="60" cy="300" r="15" fill={PLANT.leaf} />
      <circle cx="72" cy="288" r="11" fill={PLANT.leafLight} />
      <rect x="52" y="306" width="22" height="22" rx="5" fill={PLANT.pot} />
    </g>
  );
}

/** Decorative only: nobody is ever sent here. */
function BreakRoom() {
  return (
    <g>
      <rect x="36" y="560" width="230" height="104" rx="16" fill="#c8a2c0" opacity="0.55" />
      <rect x="36" y="560" width="230" height="104" rx="16" fill="none" stroke="#a97fa0" strokeWidth="4" opacity="0.7" />
      <rect x="56" y="586" width="88" height="34" rx="8" fill={WOOD.side} />
      <rect x="56" y="578" width="88" height="14" rx="7" fill={WOOD.top} />
      <rect x="68" y="554" width="30" height="28" rx="5" fill="#3a4152" />
      <rect x="73" y="560" width="20" height="10" rx="3" fill="#171b24" />
      <rect x="76" y="574" width="14" height="8" rx="2" fill="#d5dbe6" />
      <rect x="110" y="566" width="12" height="16" rx="3" fill="#e24b4a" />
      <rect x="109" y="564" width="14" height="4" rx="2" fill="#f6f2e6" />
      <circle cx="202" cy="608" r="26" fill={WOOD.side} />
      <circle cx="202" cy="604" r="24" fill={WOOD.top} />
      <circle cx="190" cy="598" r="6" fill="#f6f2e6" />
      <rect x="188" y="597" width="8" height="2.5" rx="1" fill="#6b4a2a" />
      <circle cx="214" cy="602" r="6" fill="#f6f2e6" />
      <rect x="212" y="601" width="8" height="2.5" rx="1" fill="#6b4a2a" />
      <circle cx="202" cy="616" r="5" fill="#e8b04b" />
      <rect x="232" y="576" width="18" height="20" rx="5" fill={PLANT.leaf} />
      <rect x="236" y="594" width="11" height="12" rx="3" fill={PLANT.pot} />
      <path d="M176 560 q4 -8 0 -14 q-4 -6 0 -12" stroke="#c9c2b2" strokeWidth="2.5" fill="none" strokeLinecap="round" opacity="0.8" />
      <path d="M188 564 q4 -8 0 -14 q-4 -6 0 -12" stroke="#c9c2b2" strokeWidth="2.5" fill="none" strokeLinecap="round" opacity="0.6" />
    </g>
  );
}
