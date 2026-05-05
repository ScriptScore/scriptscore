// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import { levenshtein, rosterFuzzyScore } from '$lib/intakeFuzzyMatch';
import type { LmsRosterRow } from '$lib/types';

function row(displayName: string): LmsRosterRow {
  return { userId: 'u1', displayName, sortKey: displayName };
}

describe('levenshtein', () => {
  it('handles empty strings', () => {
    expect(levenshtein('', '')).toBe(0);
    expect(levenshtein('a', '')).toBe(1);
    expect(levenshtein('', 'ab')).toBe(2);
  });

  it('counts single edits', () => {
    expect(levenshtein('a', 'b')).toBe(1);
    expect(levenshtein('cat', 'bat')).toBe(1);
  });
});

describe('rosterFuzzyScore', () => {
  it('scores truncated and noisy OCR against roster name', () => {
    const score = rosterFuzzyScore('Jord_Rivra Name:', row('Jordan Rivera'));
    expect(score).toBeGreaterThan(55);
    expect(score).toBeLessThanOrEqual(100);
  });

  it('keeps strong match for clean exact tokens', () => {
    expect(rosterFuzzyScore('Jordan Example', row('Jordan Example'))).toBe(100);
  });

  it('drops printed label tokens from hint', () => {
    const withLabel = rosterFuzzyScore('Jord Rivra Name', row('Jordan Rivera'));
    const without = rosterFuzzyScore('Jord Rivra', row('Jordan Rivera'));
    expect(Math.abs(withLabel - without)).toBeLessThanOrEqual(5);
  });
});
