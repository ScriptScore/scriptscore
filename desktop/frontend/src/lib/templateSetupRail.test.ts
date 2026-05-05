// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import { isTemplateRedactionsReadyForReview } from './templateSetupRail';

describe('templateSetupRail', () => {
  it('is ready when redaction is not required', () => {
    expect(isTemplateRedactionsReadyForReview(false, 0)).toBe(true);
  });

  it('is not ready when redaction is required and there are no regions', () => {
    expect(isTemplateRedactionsReadyForReview(true, 0)).toBe(false);
  });

  it('is ready when redaction is required and regions exist', () => {
    expect(isTemplateRedactionsReadyForReview(true, 3)).toBe(true);
  });
});
