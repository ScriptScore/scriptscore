// SPDX-License-Identifier: AGPL-3.0-only
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import PagePreviewFrame from './PagePreviewFrame.svelte';
import SelectionHandle from './SelectionHandle.svelte';
import ToneIcon from './ToneIcon.svelte';
import { ApproximatelyEqualIcon } from '@hugeicons/core-free-icons';

describe('preview and canvas primitives', () => {
  it('renders a page preview image with stable frame styling', () => {
    render(PagePreviewFrame, {
      src: '/preview.png',
      alt: 'Page 1'
    });

    const image = screen.getByRole('img', { name: 'Page 1' });
    expect(image.getAttribute('src')).toBe('/preview.png');
    expect(image.parentElement?.className).toContain('bg-surface-page');
  });

  it('renders selection handles with accessible labels', () => {
    render(SelectionHandle, {
      label: 'Resize top left'
    });

    expect(screen.getByRole('button', { name: 'Resize top left' })).toBeTruthy();
  });

  it('renders tone icons as labeled icon-only indicators', () => {
    render(ToneIcon, {
      tone: 'warning',
      icon: ApproximatelyEqualIcon,
      label: 'Rubric warning'
    });

    const icon = screen.getByLabelText('Rubric warning');
    expect(icon.className).toContain('bg-message-warning-bg');
  });
});
