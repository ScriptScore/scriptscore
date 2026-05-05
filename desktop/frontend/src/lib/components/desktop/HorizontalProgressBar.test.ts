// SPDX-License-Identifier: AGPL-3.0-only
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import HorizontalProgressBar from './HorizontalProgressBar.svelte';

describe('HorizontalProgressBar', () => {
  it('renders label, percent, and progressbar aria values', () => {
    render(HorizontalProgressBar, {
      label: 'Redact PDF',
      progress: 42,
      active: true
    });

    const bar = screen.getByRole('progressbar', { name: 'Redact PDF' });
    expect(bar.getAttribute('aria-valuemin')).toBe('0');
    expect(bar.getAttribute('aria-valuemax')).toBe('100');
    expect(bar.getAttribute('aria-valuenow')).toBe('42');
    expect(screen.getByText('42%')).toBeTruthy();
  });

  it('clamps progress values and applies the requested tone', () => {
    render(HorizontalProgressBar, {
      label: 'Graded',
      progress: 140,
      tone: 'success',
      complete: true
    });

    const bar = screen.getByRole('progressbar', { name: 'Graded' });
    expect(bar.getAttribute('aria-valuenow')).toBe('100');
    expect(bar.className).toContain('border-message-success-border');
  });

  it('can hide percent text while keeping progressbar aria values', () => {
    render(HorizontalProgressBar, {
      label: 'draft grading ready',
      progress: 100,
      complete: true,
      showPercent: false
    });

    const bar = screen.getByRole('progressbar', { name: 'draft grading ready' });
    expect(bar.getAttribute('aria-valuenow')).toBe('100');
    expect(screen.queryByText('100%')).toBeNull();
  });
});
