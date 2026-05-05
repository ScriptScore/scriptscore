// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import DesktopPopover from './DesktopPopover.svelte';

describe('DesktopPopover', () => {
  it('closes on escape and returns focus to the trigger', async () => {
    const onOpenChange = vi.fn();
    render(DesktopPopover, {
      open: true,
      triggerLabel: 'Display',
      onOpenChange
    });

    const trigger = screen.getByRole('button', { name: 'Display' });
    expect(screen.getByRole('menu')).toBeTruthy();

    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(document.activeElement).toBe(trigger);
  });

  it('closes on outside pointer down', async () => {
    const onOpenChange = vi.fn();
    render(DesktopPopover, {
      open: true,
      triggerLabel: 'Filter',
      onOpenChange
    });

    await fireEvent.pointerDown(document.body);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('supports labeled non-menu panels without changing menu defaults', () => {
    render(DesktopPopover, {
      open: true,
      triggerLabel: 'Preview tokens',
      triggerAriaHaspopup: 'dialog',
      panelRole: 'dialog',
      panelAriaLabel: 'Token guide',
      rootClass: 'relative block'
    });

    const trigger = screen.getByRole('button', { name: 'Preview tokens' });
    expect(trigger.getAttribute('aria-haspopup')).toBe('dialog');
    expect(screen.getByRole('dialog', { name: 'Token guide' })).toBeTruthy();
  });

  it('closes from marked panel controls without relying on parent state', async () => {
    const onOpenChange = vi.fn();
    render(DesktopPopover, {
      open: true,
      triggerLabel: 'Display',
      onOpenChange
    });

    const menu = screen.getByRole('menu');
    const option = document.createElement('button');
    option.type = 'button';
    option.textContent = 'Compact';
    option.setAttribute('data-popover-close', '');
    menu.append(option);

    await fireEvent.click(screen.getByRole('button', { name: 'Compact' }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(screen.queryByRole('menu')).toBeNull();
  });
});
