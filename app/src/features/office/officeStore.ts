import { useEffect, useMemo, useState } from "react";
import type { AgentView } from "../../platform/generated/AgentView";
import { readScreen } from "../../platform/terminalRegistry";
import { useAgents } from "../agents";
import { useWorkspaces } from "../workspaces";
import { occupants, podCount, seat, type Seating } from "./floor";
import { labelFor, nameStaff, personaOf, roleOf, type Persona } from "./persona";
import { lastLines } from "./screen";

/** An agent as the office shows it. */
export interface Employee {
  agent: AgentView;
  persona: Persona;
  role: string;
  /** Pod number, from 0 along the rows. */
  pod: number;
}

export interface Office {
  employees: Employee[];
  /** Pod by pod, who is in it; null for a vacant one. */
  pods: (Employee | null)[];
}

/**
 * Where everyone sat a moment ago, per workspace. Outside React on purpose: a
 * pane that is closed and reopened, or remounted by a layout change, must find
 * people where it left them.
 */
const seatings = new Map<string, Seating>();
/** And what everyone was called, kept for the same reason. */
const namings = new Map<string, ReadonlyMap<string, string>>();

/**
 * What the office calls an agent, if it has met them: a name exists once an
 * office pane has shown that workspace. Until then there is nobody to have
 * heard the name from, and callers say "Claude" as they always did.
 */
export function officeNameOf(agentId: string): string | undefined {
  for (const names of namings.values()) {
    const name = names.get(agentId);
    if (name) return name;
  }
  return undefined;
}

/** The workspace's office: its living agents, seated. */
export function useOffice(workspaceId: string, maxConcurrent: number): Office {
  const list = useAgents();
  const panes = useWorkspaces()?.workspaces.find((ws) => ws.id === workspaceId)?.panes;
  return useMemo(() => {
    const present = occupants(list?.agents ?? [], workspaceId);
    const seating = seat(seatings.get(workspaceId) ?? new Map(), present);
    seatings.set(workspaceId, seating);
    const names = nameStaff(namings.get(workspaceId) ?? new Map(), present);
    namings.set(workspaceId, names);
    const employees = present.map((agent) => ({
      agent,
      persona: { ...personaOf(agent.id), name: names.get(agent.id) ?? personaOf(agent.id).name },
      role: roleOf({ ...agent, label: labelFor(agent, panes) }, present),
      pod: seating.get(agent.id) ?? 0,
    }));
    const pods: (Employee | null)[] = Array.from({ length: podCount(seating, maxConcurrent) }, () => null);
    for (const employee of employees) pods[employee.pod] = employee;
    return { employees, pods };
  }, [list, panes, workspaceId, maxConcurrent]);
}

/** How often a screen is looked at. Slow enough to be free, fast enough to feel live. */
const GLANCE_MS = 1000;

/**
 * The last `lines` lines on each agent's screen, by agent id. Polled, because a
 * terminal announces nothing when it draws; state only changes when a screen
 * does, so a quiet office does not re-render.
 */
export function useScreens(employees: readonly Employee[], lines: number): ReadonlyMap<string, string[]> {
  const [screens, setScreens] = useState<ReadonlyMap<string, string[]>>(new Map());
  const panes = employees.map(({ agent }) => `${agent.id}=${agent.pane_id ?? ""}`).join(",");
  useEffect(() => {
    const glance = () => {
      const next = new Map<string, string[]>();
      for (const pair of panes.split(",")) {
        const [id, pane] = pair.split("=");
        if (id && pane) next.set(id, lastLines(readScreen(pane), lines));
      }
      setScreens((current) => (sameScreens(current, next) ? current : next));
    };
    glance();
    const timer = setInterval(glance, GLANCE_MS);
    return () => clearInterval(timer);
  }, [panes, lines]);
  return screens;
}

function sameScreens(a: ReadonlyMap<string, string[]>, b: ReadonlyMap<string, string[]>): boolean {
  if (a.size !== b.size) return false;
  for (const [id, lines] of b) {
    if (a.get(id)?.join("\n") !== lines.join("\n")) return false;
  }
  return true;
}
