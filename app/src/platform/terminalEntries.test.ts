import { describe, expect, it } from "vitest";
import { MirrorFeed } from "./mirrorFeed";
import { type Entry, entries, retire } from "./terminalEntries";

describe("retiring a pane's terminal", () => {
  it("takes it off the map before its mirrors hear, so one that looks again finds nothing", () => {
    const found: boolean[] = [];
    const mirrors = new Set<MirrorFeed>();
    mirrors.add(new MirrorFeed((message) => message.kind === "end" && found.push(entries.has("p1"))));
    mirrors.add(new MirrorFeed((message) => message.kind === "end" && found.push(entries.has("p1"))));
    entries.set("p1", { paneId: "p1", mirrors } as unknown as Entry);

    const entry = retire("p1");

    expect(entry?.paneId).toBe("p1");
    expect(found).toEqual([false, false]);
    expect(mirrors.size).toBe(0);
  });

  it("does nothing for a pane it does not hold", () => {
    expect(retire("nobody")).toBeUndefined();
  });
});
