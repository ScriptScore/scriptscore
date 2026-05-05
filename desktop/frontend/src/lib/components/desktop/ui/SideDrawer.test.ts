// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import SideDrawer from './SideDrawer.svelte';

describe('SideDrawer', () => {
  it('renders dialog semantics and closes on escape', async () => {
    const onClose = vi.fn();
    render(SideDrawer, {
      open: true,
      overlay: true,
      title: 'Trace',
      description: 'Runtime details',
      onClose
    });

    const drawer = screen.getByRole('dialog', { name: 'Trace' });
    expect(drawer.getAttribute('aria-modal')).toBe('true');
    expect(screen.getByText('Runtime details')).toBeTruthy();

    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
