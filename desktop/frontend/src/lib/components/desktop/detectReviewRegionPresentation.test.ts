// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import { detectReviewRegionPresentation } from './detectReviewRegionPresentation';

describe('detectReviewRegionPresentation', () => {
  it('uses a translucent question-region fill over page content', () => {
    const presentation = detectReviewRegionPresentation(
      {
        regionId: 'q1:1',
        pageNumber: 1,
        x: 10,
        y: 10,
        width: 80,
        height: 40,
        kind: 'template'
      },
      0
    );

    expect(presentation.fillColor).toBe('var(--message-warning-selection-fill)');
    expect(presentation.fillColor).not.toBe('var(--message-warning-bg)');
  });

  it('keeps resolved labels distinct from seeded regions', () => {
    expect(
      detectReviewRegionPresentation(
        {
          regionId: 'q1:1',
          pageNumber: 1,
          x: 10,
          y: 10,
          width: 80,
          height: 40,
          kind: 'resolved'
        },
        0
      ).label
    ).toBe('Resolved');
  });
});
