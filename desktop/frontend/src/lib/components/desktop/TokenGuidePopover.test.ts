// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import TokenGuidePopover from './TokenGuidePopover.svelte';

describe('TokenGuidePopover', () => {
  it('opens the guide image and closes with shared popover behavior', async () => {
    render(TokenGuidePopover, {
      label: 'Not sure how?',
      imageSrc: '/token-guide.png',
      imageAlt: 'Token guide screenshot'
    });

    const trigger = screen.getByRole('button', { name: 'Not sure how?' });
    await fireEvent.click(trigger);

    expect(screen.getByRole('dialog', { name: 'Token guide screenshot' })).toBeTruthy();
    expect(screen.getByRole('img', { name: 'Token guide screenshot' })).toBeTruthy();

    await fireEvent.keyDown(document, { key: 'Escape' });

    expect(screen.queryByRole('dialog', { name: 'Token guide screenshot' })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('closes on outside pointer interaction', async () => {
    render(TokenGuidePopover, {
      label: 'Show token guide',
      imageSrc: '/token-guide.png',
      imageAlt: 'Token guide screenshot'
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Show token guide' }));
    expect(screen.getByRole('dialog', { name: 'Token guide screenshot' })).toBeTruthy();

    await fireEvent.pointerDown(document.body);

    expect(screen.queryByRole('dialog', { name: 'Token guide screenshot' })).toBeNull();
  });
});
