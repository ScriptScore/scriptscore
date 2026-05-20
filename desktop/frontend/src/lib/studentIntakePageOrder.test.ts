// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import { normalizeDesiredPageOrder, reorderPageNumbers } from './studentIntakePageOrder';

describe('student intake page order helpers', () => {
  it('uses the natural preview order when no desired order is supplied', () => {
    expect(normalizeDesiredPageOrder([1, 2, 3], undefined)).toEqual([1, 2, 3]);
  });

  it('keeps a valid desired order', () => {
    expect(normalizeDesiredPageOrder([1, 2, 3], [3, 1, 2])).toEqual([3, 1, 2]);
  });

  it('keeps a partial selected page order', () => {
    expect(normalizeDesiredPageOrder([1, 2, 3], [3, 1])).toEqual([3, 1]);
  });

  it('ignores invalid desired pages without restoring excluded pages', () => {
    expect(normalizeDesiredPageOrder([1, 2, 3], [3, 99, 3, 0, 1])).toEqual([3, 1]);
  });

  it('reorders page numbers by drag target', () => {
    expect(reorderPageNumbers([1, 2, 3, 4], 4, 2)).toEqual([1, 4, 2, 3]);
  });
});
