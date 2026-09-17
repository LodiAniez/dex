import type { Persona } from "./persona";

/** Head and shoulders, for a card or a panel header. */
export function Avatar({ persona, size }: { persona: Persona; size: number }) {
  return (
    <svg width={size} height={Math.round(size * 1.05)} viewBox="0 0 40 42" aria-hidden="true">
      <path d="M4 42 v-8 a16 16 0 0 1 32 0 v8 z" fill={persona.shirt} />
      <circle cx="20" cy="14" r="11" fill={persona.skin} />
      <ellipse cx="20" cy="6" rx="12" ry="5.5" fill={persona.hair} />
    </svg>
  );
}
