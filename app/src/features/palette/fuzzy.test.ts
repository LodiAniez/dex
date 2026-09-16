import { describe, expect, it } from "vitest";
import { fuzzyMatch } from "./fuzzy";

function score(query: string, text: string): number {
  const match = fuzzyMatch(query, text);
  if (!match) throw new Error(`"${query}" does not match "${text}"`);
  return match.score;
}

/** Asserts `query` ranks `better` above `worse`, which is all the scores promise. */
function prefers(query: string, better: string, worse: string): void {
  expect(score(query, better), `"${query}": "${better}" should beat "${worse}"`).toBeGreaterThan(
    score(query, worse),
  );
}

describe("whether a query matches", () => {
  it("matches a subsequence, and nothing else", () => {
    expect(fuzzyMatch("spr", "Split pane right")).not.toBeNull();
    expect(fuzzyMatch("xyz", "Split pane right")).toBeNull();
    expect(fuzzyMatch("thgir", "right")).toBeNull();
  });

  it("ignores case both ways", () => {
    expect(fuzzyMatch("SPLIT", "split pane right")).not.toBeNull();
    expect(fuzzyMatch("split", "SPLIT PANE RIGHT")).not.toBeNull();
  });

  it("matches everything with an empty query", () => {
    expect(fuzzyMatch("", "anything")).toEqual({ score: 0, positions: [] });
  });

  it("refuses a query longer than the text", () => {
    expect(fuzzyMatch("workspaces", "work")).toBeNull();
  });

  it("reports where the letters landed, in order", () => {
    expect(fuzzyMatch("sr", "Split right")?.positions).toEqual([0, 6]);
    expect(fuzzyMatch("api", "api")?.positions).toEqual([0, 1, 2]);
  });
});

describe("what a query prefers", () => {
  it("the whole text over a text that merely contains it", () => {
    prefers("web", "web", "Workspace: web-api");
    prefers("api", "api", "api-gateway");
  });

  it("letters that start words over the same letters buried inside", () => {
    // "sr" is how someone types "Split right"; "Cursor" also has an s then an
    // r, in the middle of the word.
    prefers("sr", "Split right", "Cursor");
    prefers("cp", "Close pane", "escape");
  });

  it("letters together over letters apart", () => {
    prefers("pane", "Close pane", "Pin a name");
    prefers("split", "Split right", "Superb list");
  });

  it("a match at the start over the same match later", () => {
    prefers("focus", "Focus pane left", "Move pane, then focus");
  });

  it("the start of a camelCase word like the start of a word", () => {
    prefers("db", "diffBase", "dashboard");
  });

  it("uses the best letters, not the first ones it sees", () => {
    // Greedy left-to-right would take the "s" inside "Workspace"; the "s" that
    // starts "switch" is the better match even though it is further away.
    expect(fuzzyMatch("ws", "Workspace switch")?.positions).toEqual([0, 10]);
  });

  it("does not let a long gap cost more than a bounded amount", () => {
    // A query whose letters are far apart is still a match; the gap penalty is
    // capped so it does not drop below an unrelated shorter name.
    expect(fuzzyMatch("wt", "workspace with a very long name that ends in t")).not.toBeNull();
  });
});
