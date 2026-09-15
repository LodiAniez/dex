import { invoke } from "@tauri-apps/api/core";
import type { ErrorBody } from "./generated/ErrorBody";

/** One daemon response; mirrors `dex_protocol::Response`. */
interface Response<T> {
  id: string;
  ok: boolean;
  data?: T;
  error?: ErrorBody;
}

/** A failed daemon command, with the repair string the user should see. */
export class DexError extends Error {
  readonly code: ErrorBody["code"];
  readonly repair: string;

  constructor(body: ErrorBody) {
    super(body.message);
    this.code = body.code;
    this.repair = body.repair;
  }
}

let nextId = 0;

/**
 * Runs a daemon command. The UI uses the same request envelope the CLI sends
 * over the pipe (PRD §6.2), so both reach the same handlers.
 */
export async function request<T>(cmd: string, args: object = {}): Promise<T> {
  nextId += 1;
  const response = await invoke<Response<T>>("dex_request", {
    request: { id: `ui-${nextId}`, cmd, args },
  });
  if (!response.ok) {
    throw new DexError(
      response.error ?? { code: "internal", message: `${cmd} failed without an error`, repair: "This is a bug in Dex." },
    );
  }
  return response.data as T;
}
