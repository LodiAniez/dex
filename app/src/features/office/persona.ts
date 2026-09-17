/**
 * Who an agent is in the office: a name and a look, worked out from its id and
 * nothing else. Nothing is stored, so a persona survives a restart and is the
 * same in every view — which also means the hash and the order of these lists
 * are permanent. Append to a list and some agents change; reorder one and they
 * all do.
 */

export interface Persona {
  name: string;
  skin: string;
  hair: string;
  shirt: string;
  /** The rug under the desk: the shirt's tint. */
  rug: string;
  rugLine: string;
}

export const NAMES = [
  "Morgan", "Juno", "Pip", "Ravi", "Tess", "Olu", "Kai", "Mina",
  "Nico", "Wren", "Sol", "Ada", "Bo", "Cleo", "Dara", "Eli",
  "Fen", "Gus", "Hana", "Ira", "Jem", "Kit", "Lux", "Milo",
  "Noor", "Otis", "Pax", "Quin", "Remy", "Suki", "Theo", "Uma",
  "Vik", "Yara", "Zed", "Alba", "Beck", "Cyd", "Dov", "Esme",
  "Finn", "Gia", "Hugo", "Ines", "Jude", "Koa", "Lior", "Maz",
  "Nell", "Oren", "Priya", "Rue", "Sami", "Tova", "Ugo", "Vera",
  "Wes", "Xan", "Yuki", "Zora", "Ari", "Bea", "Cass", "Dee",
] as const;

export const SKINS = ["#f2c9a0", "#c68642", "#f8d8b8", "#8d5a3b"] as const;

export const HAIRS = ["#5b3a1e", "#1c1c1c", "#b0562c", "#101010", "#7a2c48", "#2c2c2a"] as const;

/** A shirt and the rug that goes with it. */
export const SHIRTS = [
  { shirt: "#185fa5", rug: "#bcd7f2", rugLine: "#8fb4dd" },
  { shirt: "#993c1d", rug: "#f3c9b6", rugLine: "#dba088" },
  { shirt: "#0f6e56", rug: "#b7e4d6", rugLine: "#84c2ae" },
  { shirt: "#534ab7", rug: "#d4d0f2", rugLine: "#aaa4e0" },
  { shirt: "#993556", rug: "#f2c7d6", rugLine: "#d89cb2" },
  { shirt: "#ba7517", rug: "#f4dcae", rugLine: "#d9b578" },
] as const;

/** FNV-1a, 32 bits, with a final mix so that similar ids spread out. */
function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  h ^= h >>> 16;
  h = Math.imul(h, 0x85ebca6b);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return h >>> 0;
}

/** One trait, hashed with its own salt so that no trait decides another. */
function pick<T>(id: string, trait: string, from: readonly T[]): T {
  return from[hash(`${trait}:${id}`) % from.length];
}

export function personaOf(id: string): Persona {
  return {
    name: pick(id, "name", NAMES),
    skin: pick(id, "skin", SKINS),
    hair: pick(id, "hair", HAIRS),
    ...pick(id, "shirt", SHIRTS),
  };
}

/**
 * Names for the people present, no two alike. An id asks for a name; if someone
 * who arrived earlier has it, the newcomer takes the next one along that nobody
 * is using. `previous` is who was called what a moment ago, and wins: nobody is
 * renamed because a namesake left. In an office larger than the list, the
 * overflow share names — the label beside each still tells them apart.
 */
export function nameStaff(
  previous: ReadonlyMap<string, string>,
  present: readonly { id: string }[],
): ReadonlyMap<string, string> {
  const names = new Map<string, string>();
  for (const { id } of present) {
    const kept = previous.get(id);
    if (kept) names.set(id, kept);
  }
  const taken = new Set(names.values());
  for (const { id } of present) {
    if (names.has(id)) continue;
    const pool: readonly string[] = NAMES;
    const wanted = pool.indexOf(personaOf(id).name);
    let name: string = NAMES[wanted];
    for (let step = 0; step < NAMES.length; step += 1) {
      const candidate = NAMES[(wanted + step) % NAMES.length];
      if (!taken.has(candidate)) {
        name = candidate;
        break;
      }
    }
    names.set(id, name);
    taken.add(name);
  }
  return names;
}

/**
 * What an agent is called by whoever set it up: the label it was spawned with,
 * else the label of the pane it runs in. The second is the common one — an
 * agent the owner started themselves has no label of its own, but they named
 * the pane.
 */
export function labelFor(
  agent: { label: string | null; pane_id: string | null },
  panes: readonly { id: string; label: string | null }[] | undefined,
): string | null {
  return agent.label ?? panes?.find((pane) => pane.id === agent.pane_id)?.label ?? null;
}

/**
 * Every name the daemon may use for an agent when it writes an event: the label
 * it was spawned with, its pane's label, and - for an agent with neither - the
 * first segment of its id, which is what the daemon falls back to.
 */
export function knownAs(
  agent: { id: string; label: string | null; pane_id: string | null },
  panes: readonly { id: string; label: string | null }[] | undefined,
): string[] {
  const pane = panes?.find((candidate) => candidate.id === agent.pane_id)?.label ?? null;
  const names = [agent.label, pane, agent.id.split("-")[0]];
  return names.filter((name): name is string => !!name);
}

/** The longest role a name tag has room for. */
const ROLE_CHARS = 24;
const ROLE_WORDS = 3;

interface Lineage {
  id: string;
  parent_id: string | null;
}

/**
 * What an agent does here, in a few words: "lead" for one a human started that
 * has spawned others, else its label, else the start of its brief.
 */
export function roleOf(
  agent: Lineage & { label: string | null; task_brief: string | null },
  everyone: readonly Lineage[],
): string {
  if (agent.parent_id === null && everyone.some((other) => other.parent_id === agent.id)) {
    return "lead";
  }
  const label = agent.label?.trim();
  if (label) return label;
  const words = agent.task_brief?.trim().split(/\s+/).filter(Boolean) ?? [];
  if (words.length === 0) return "agent";
  return words.slice(0, ROLE_WORDS).join(" ").toLowerCase().slice(0, ROLE_CHARS).trimEnd();
}
