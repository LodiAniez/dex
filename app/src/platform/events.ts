import { listen } from "@tauri-apps/api/event";

/** Emitted by the app when daemon state changes; the payload is the topic ("all" means everything). */
const CHANGED_EVENT = "dex://changed";

/** Calls `onChange` whenever the daemon reports a change to `topic` — for example one made from the CLI. */
export function onDaemonChange(topic: string, onChange: () => void): Promise<() => void> {
  return listen<string>(CHANGED_EVENT, (event) => {
    if (event.payload === topic || event.payload === "all") onChange();
  });
}
