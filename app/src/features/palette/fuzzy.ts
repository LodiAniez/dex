/**
 * Fuzzy matching for the palette: does `query` appear in `text` as a
 * subsequence, and how good a match is it?
 *
 * The scoring is about what people type. "sr" should find "Split right" ahead
 * of "Workspace 1", because the letters start words; "web" should find "web"
 * ahead of "Workspace: web-api", because it is the whole thing; and a query
 * whose letters are scattered through a long name should lose to one where
 * they sit together. Numbers are tuned only relative to each other — the tests
 * assert orderings, never scores.
 */

/** A successful match: higher `score` is better, `positions` are the matched indexes in `text`. */
export interface Match {
  score: number;
  positions: number[];
}

const AT_START = 12;
const AFTER_SEPARATOR = 9;
const CAMEL_HUMP = 7;
const CONSECUTIVE = 6;
const ANYWHERE = 1;
/** Per skipped character between two matched ones, up to `GAP_CAP`. */
const GAP = -1;
const GAP_CAP = -6;
/** A query that is the whole text, ignoring case, beats every other match. */
const EXACT = 40;

const SEPARATORS = new Set([" ", "-", "_", "/", "\\", ".", ":", "(", ")"]);

function isUpper(ch: string): boolean {
  return ch !== ch.toLowerCase() && ch === ch.toUpperCase();
}

function isLower(ch: string): boolean {
  return ch !== ch.toUpperCase() && ch === ch.toLowerCase();
}

/** How much matching `text[at]` is worth on its own, before gaps and runs. */
function positional(text: string, at: number): number {
  if (at === 0) return AT_START;
  const before = text[at - 1];
  if (SEPARATORS.has(before)) return AFTER_SEPARATOR;
  if (isUpper(text[at]) && isLower(before)) return CAMEL_HUMP;
  return ANYWHERE;
}

/**
 * The best match of `query` in `text`, or null if it is not a subsequence.
 * Case-insensitive. An empty query matches everything with a score of zero.
 */
export function fuzzyMatch(query: string, text: string): Match | null {
  if (query.length === 0) return { score: 0, positions: [] };
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (q.length > t.length) return null;
  if (q === t) return { score: EXACT + AT_START * q.length, positions: [...q].map((_, i) => i) };

  // Every occurrence of each query character in the text, so the search
  // below tries each real option rather than only the first.
  const occurrences: number[][] = [];
  for (const ch of q) {
    const at: number[] = [];
    for (let i = 0; i < t.length; i += 1) if (t[i] === ch) at.push(i);
    if (at.length === 0) return null;
    occurrences.push(at);
  }

  // Best score for matching query[qi..] with text positions at or after
  // `from`, where `previous` is the position just matched. Memoized on
  // (qi, from), which is sound because `from` is always `previous + 1`.
  // Texts and queries are short, so this is cheap, and it is what lets "sr"
  // prefer the S and R that start words over the first S and R it sees.
  const memo = new Map<number, { score: number; positions: number[] } | null>();
  const best = (qi: number, from: number, previous: number): Match | null => {
    if (qi === q.length) return { score: 0, positions: [] };
    const key = qi * (t.length + 1) + from;
    const cached = memo.get(key);
    if (cached !== undefined) return cached;

    let top: Match | null = null;
    for (const at of occurrences[qi]) {
      if (at < from) continue;
      const rest = best(qi + 1, at + 1, at);
      if (!rest) continue;
      let here = positional(text, at);
      if (previous >= 0) {
        const gap = at - previous - 1;
        here += gap === 0 ? CONSECUTIVE : Math.max(GAP_CAP, GAP * gap);
      }
      const score = here + rest.score;
      if (!top || score > top.score) top = { score, positions: [at, ...rest.positions] };
    }
    memo.set(key, top);
    return top;
  };
  return best(0, 0, -1);
}
