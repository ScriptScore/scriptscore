// SPDX-License-Identifier: AGPL-3.0-only
import type { LmsRosterRow } from '$lib/types';

/** Printed form labels often picked up by OCR; exclude from matching. */
const NOISE_WORDS = new Set([
  'name',
  'student',
  'id',
  'date',
  'signature',
  'section',
  'course',
  'first',
  'last',
  'middle',
  'grade',
  'score',
  'total',
  'page',
  'exam',
  'test',
  'quiz',
  'assignment'
]);

export function normalizeForMatch(s: string): string {
  return s.toLowerCase().replaceAll(/[^a-z0-9]+/g, ' ').trim();
}

function stripNoiseTokens(normalized: string): string {
  const parts = normalized.split(/\s+/).filter((p) => p.length > 0);
  return parts.filter((p) => !NOISE_WORDS.has(p)).join(' ');
}

/** Levenshtein distance (two-row DP, O(min(m,n)) space). */
export function levenshtein(a: string, b: string): number {
  const m = a.length;
  const n = b.length;
  if (m === 0) return n;
  if (n === 0) return m;
  const prev = new Array<number>(n + 1);
  const cur = new Array<number>(n + 1);
  for (let j = 0; j <= n; j++) prev[j] = j;
  for (let i = 1; i <= m; i++) {
    cur[0] = i;
    for (let j = 1; j <= n; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      const diag = prev[j - 1] ?? 0;
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, diag + cost);
    }
    for (let j = 0; j <= n; j++) {
      const v = cur[j];
      if (v !== undefined) prev[j] = v;
    }
  }
  return prev[n] ?? 0;
}

/** 0..1 similarity from Levenshtein (1 = identical). */
export function stringSimilarity(a: string, b: string): number {
  if (!a || !b) return 0;
  if (a === b) return 1;
  const dist = levenshtein(a, b);
  const denom = Math.max(a.length, b.length);
  return denom === 0 ? 1 : Math.max(0, 1 - dist / denom);
}

const MIN_PREFIX_HINT_LEN = 3;

/**
 * Best similarity between one OCR token and roster name tokens.
 * Uses edit distance, prefix match (truncated OCR), and substring includes.
 */
export function bestTokenSimilarity(hintToken: string, nameTokens: string[]): number {
  if (hintToken.length < 2) return 0;
  let best = 0;
  for (const nt of nameTokens) {
    if (nt.length < 1) continue;
    if (hintToken.length >= MIN_PREFIX_HINT_LEN && nt.startsWith(hintToken)) {
      best = Math.max(best, 0.82 + 0.18 * (hintToken.length / nt.length));
    }
    if (nt.includes(hintToken) && hintToken.length >= MIN_PREFIX_HINT_LEN) {
      best = Math.max(best, 0.75 + 0.2 * (hintToken.length / nt.length));
    }
    if (hintToken.includes(nt) && nt.length >= MIN_PREFIX_HINT_LEN) {
      best = Math.max(best, 0.75 + 0.2 * (nt.length / hintToken.length));
    }
    best = Math.max(best, stringSimilarity(hintToken, nt));
  }
  return best;
}

/**
 * Score 0..100: OCR hint vs LMS roster display name.
 * Tolerates label noise (e.g. "Name:"), typos, and truncated tokens ("Dens" vs "Rivera").
 */
export function rosterFuzzyScore(hint: string, row: LmsRosterRow): number {
  const raw = normalizeForMatch(hint);
  const cleaned = stripNoiseTokens(raw);
  const h = cleaned.trim();
  if (!h) return 0;
  const display = normalizeForMatch(row.displayName);
  if (!display) return 0;

  const fullSim = stringSimilarity(h, display);

  const hintTokens = h.split(/\s+/).filter((t) => t.length >= 2);
  const nameTokens = display.split(/\s+/).filter((t) => t.length >= 1);

  let tokenScore = 0;
  if (hintTokens.length > 0 && nameTokens.length > 0) {
    let sum = 0;
    for (const ht of hintTokens) {
      sum += bestTokenSimilarity(ht, nameTokens);
    }
    tokenScore = sum / hintTokens.length;
  }

  const legacyIncludes = legacySubstringScore(h, display);

  const combined = Math.max(fullSim, tokenScore, legacyIncludes);
  return Math.min(100, Math.round(combined * 100));
}

/** Previous behavior: count hint words that appear as substrings in the display name. */
function legacySubstringScore(h: string, display: string): number {
  const parts = h.split(/\s+/).filter((p) => p.length >= 2);
  if (parts.length === 0) return 0;
  let matched = 0;
  for (const part of parts) {
    if (display.includes(part)) matched += 1;
  }
  return matched / parts.length;
}
