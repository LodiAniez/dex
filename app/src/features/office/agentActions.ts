import { ask } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { request } from "../../platform/daemon";
import { showError } from "../../platform/notices";
import { clockOutArgs, clockOutQuestion } from "./hire";
import type { Employee } from "./officeStore";
import { promptArgs } from "./prompt";

/**
 * What the owner can do to an agent from the office, wherever the button is:
 * on their card, or in their panel. One place, so both behave the same.
 */
export function useAgentActions(employee: Employee) {
  const { agent, persona, role } = employee;
  const [sending, setSending] = useState(false);
  const [leaving, setLeaving] = useState(false);

  /** Types `text` at the agent as the owner's turn. Resolves to whether it was sent. */
  const prompt = async (text: string): Promise<boolean> => {
    const args = promptArgs(agent.id, text);
    if (!args || sending) return false;
    setSending(true);
    try {
      // The daemon decides whether it is safe to type there, and says why not.
      await request("agent.prompt", args);
      return true;
    } catch (err) {
      showError(err);
      return false;
    } finally {
      setSending(false);
    }
  };

  /** After a confirmation: `/exit`, then their pane. They walk out by themselves. */
  const clockOut = async (): Promise<void> => {
    if (!(await ask(clockOutQuestion(persona.name, role), { title: "Clock out", kind: "warning" }))) return;
    setLeaving(true);
    try {
      await request("agent.stop", clockOutArgs(agent.id));
    } catch (err) {
      showError(err);
      setLeaving(false);
    }
  };

  return { prompt, sending, clockOut, leaving };
}
