import { describe, expect, it } from "vitest";
import { HAIRS, NAMES, SHIRTS, SKINS, labelFor, nameStaff, personaOf, roleOf } from "./persona";

/** Ids shaped like the daemon's: a prefix and hex. */
const ids = Array.from({ length: 4000 }, (_, i) => `agent-${(i * 2654435761).toString(16)}`);

describe("personaOf", () => {
  it("gives one agent the same persona every time", () => {
    expect(personaOf("agent-7f3a")).toEqual(personaOf("agent-7f3a"));
  });

  it("is fixed across releases, because a renamed colleague is a stranger", () => {
    // Pinned on purpose. If this fails, the hash or the order of a list
    // changed, and every agent anyone is looking at became somebody else.
    expect(personaOf("a")).toEqual({
      name: "Jude",
      skin: "#8d5a3b",
      hair: "#b0562c",
      shirt: "#ba7517",
      rug: "#f4dcae",
      rugLine: "#d9b578",
    });
    expect(personaOf("agent-7f3a").name).toBe("Tova");
  });

  it("tells different agents apart", () => {
    const distinct = new Set(ids.slice(0, 200).map((id) => JSON.stringify(personaOf(id))));
    expect(distinct.size).toBeGreaterThan(150);
  });

  it("can reach every name and every colour", () => {
    const seen = { names: new Set<string>(), skins: new Set<string>(), hairs: new Set<string>(), shirts: new Set<string>() };
    for (const id of ids) {
      const persona = personaOf(id);
      seen.names.add(persona.name);
      seen.skins.add(persona.skin);
      seen.hairs.add(persona.hair);
      seen.shirts.add(persona.shirt);
    }
    expect(seen.names.size).toBe(NAMES.length);
    expect(seen.skins.size).toBe(SKINS.length);
    expect(seen.hairs.size).toBe(HAIRS.length);
    expect(seen.shirts.size).toBe(SHIRTS.length);
  });

  it("lays the rug that goes with the shirt", () => {
    for (const id of ids.slice(0, 300)) {
      const persona = personaOf(id);
      const set = SHIRTS.find((s) => s.shirt === persona.shirt);
      expect(set, persona.shirt).toBeDefined();
      expect(persona.rug).toBe(set?.rug);
      expect(persona.rugLine).toBe(set?.rugLine);
    }
  });

  it("chooses each trait on its own, so a shirt does not decide a face", () => {
    const skinsInBlue = new Set(
      ids.map(personaOf).filter((p) => p.shirt === SHIRTS[0].shirt).map((p) => p.skin),
    );
    expect(skinsInBlue.size).toBe(SKINS.length);
  });

  it("uses names that are unique and short enough for a name tag", () => {
    expect(new Set(NAMES).size).toBe(NAMES.length);
    for (const name of NAMES) expect(name.length, name).toBeLessThanOrEqual(8);
  });
});

describe("roleOf", () => {
  const agent = (id: string, extra: Partial<{ label: string | null; task_brief: string | null; parent_id: string | null }> = {}) => ({
    id,
    label: null,
    task_brief: null,
    parent_id: null,
    ...extra,
  });

  it("calls an agent with children the lead, whatever its label", () => {
    const boss = agent("boss", { label: "main" });
    const all = [boss, agent("kid", { parent_id: "boss" })];
    expect(roleOf(boss, all)).toBe("lead");
  });

  it("does not call a childless root a lead", () => {
    const solo = agent("solo", { label: "server" });
    expect(roleOf(solo, [solo])).toBe("server");
  });

  it("does not call a child with children of its own the lead", () => {
    const middle = agent("mid", { parent_id: "boss", label: "porter" });
    const all = [agent("boss"), middle, agent("leaf", { parent_id: "mid" })];
    expect(roleOf(middle, all)).toBe("porter");
  });

  it("falls back to the first words of the brief", () => {
    const worker = agent("w", { task_brief: "Port the auth module to the new session API, then run the tests" });
    expect(roleOf(worker, [worker])).toBe("port the auth");
  });

  it("keeps a long first word from overflowing the tag", () => {
    const worker = agent("w", { task_brief: "Internationalisationalise everything now" });
    expect(roleOf(worker, [worker]).length).toBeLessThanOrEqual(24);
  });

  it("treats a blank label or brief as missing", () => {
    const blank = agent("b", { label: "  ", task_brief: "\n " });
    expect(roleOf(blank, [blank])).toBe("agent");
  });
});

describe("nameStaff", () => {
  // Two ids that hash to the same name, found rather than assumed.
  const [first, twin] = (() => {
    const seen = new Map<string, string>();
    for (const id of ids) {
      const name = personaOf(id).name;
      const other = seen.get(name);
      if (other) return [other, id];
      seen.set(name, id);
    }
    throw new Error("no two ids share a name");
  })();

  it("gives everyone the name their id asks for when nobody shares it", () => {
    const names = nameStaff(new Map(), [{ id: "a" }, { id: "agent-7f3a" }]);
    expect(names.get("a")).toBe("Jude");
    expect(names.get("agent-7f3a")).toBe("Tova");
  });

  it("never has two people of one name in the office", () => {
    const names = nameStaff(new Map(), [{ id: first }, { id: twin }]);
    expect(names.get(first)).toBe(personaOf(first).name);
    expect(names.get(twin)).not.toBe(names.get(first));
    expect(NAMES).toContain(names.get(twin));
  });

  it("lets whoever arrived first keep the name", () => {
    const names = nameStaff(new Map(), [{ id: twin }, { id: first }]);
    expect(names.get(twin)).toBe(personaOf(twin).name);
    expect(names.get(first)).not.toBe(names.get(twin));
  });

  it("does not rename anyone when their namesake leaves", () => {
    const before = nameStaff(new Map(), [{ id: first }, { id: twin }]);
    const after = nameStaff(before, [{ id: twin }]);
    expect(after.get(twin)).toBe(before.get(twin));
    expect(after.has(first)).toBe(false);
  });

  it("still names everyone in an office bigger than the list of names", () => {
    const crowd = ids.slice(0, NAMES.length + 5).map((id) => ({ id }));
    const names = nameStaff(new Map(), crowd);
    expect(names.size).toBe(crowd.length);
    for (const name of names.values()) expect(name).toBeTruthy();
  });
});

describe("labelFor", () => {
  const panes = [
    { id: "p1", label: "main" },
    { id: "p2", label: null },
  ];

  it("is the agent's own label when it was spawned with one", () => {
    expect(labelFor({ label: "porter", pane_id: "p1" }, panes)).toBe("porter");
  });

  it("is otherwise the label of the pane it runs in, which is what its owner called it", () => {
    expect(labelFor({ label: null, pane_id: "p1" }, panes)).toBe("main");
  });

  it("is nothing for an unlabelled pane, a closed one, or a workspace not loaded yet", () => {
    expect(labelFor({ label: null, pane_id: "p2" }, panes)).toBeNull();
    expect(labelFor({ label: null, pane_id: "gone" }, panes)).toBeNull();
    expect(labelFor({ label: null, pane_id: null }, panes)).toBeNull();
    expect(labelFor({ label: null, pane_id: "p1" }, undefined)).toBeNull();
  });
});
