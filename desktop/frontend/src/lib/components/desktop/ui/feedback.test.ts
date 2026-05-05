// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import { feedbackRole, feedbackToneClass } from './feedback';

describe('desktop feedback helpers', () => {
  it('maps urgent tones to alert semantics', () => {
    expect(feedbackRole('warning')).toBe('alert');
    expect(feedbackRole('error')).toBe('alert');
    expect(feedbackRole('success')).toBe('status');
    expect(feedbackRole('info')).toBe('status');
  });

  it('returns semantic token classes for tone styling', () => {
    expect(feedbackToneClass('success')).toContain('message-success');
    expect(feedbackToneClass('info')).toContain('message-info');
    expect(feedbackToneClass('warning')).toContain('message-warning');
    expect(feedbackToneClass('error')).toContain('message-error');
  });
});
