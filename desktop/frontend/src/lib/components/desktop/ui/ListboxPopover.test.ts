// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ListboxPopover from './ListboxPopover.svelte';

const options = [
  { value: 'name', label: 'Name' },
  { value: 'score', label: 'Score' },
  { value: 'status', label: 'Status', disabled: true }
];

describe('ListboxPopover', () => {
  it('opens from the trigger and selects an enabled option', async () => {
    const onOpenChange = vi.fn();
    const onSelect = vi.fn();

    render(ListboxPopover, {
      triggerLabel: 'Sort students',
      ariaLabel: 'Sort options',
      value: 'name',
      options,
      onOpenChange,
      onSelect
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Sort students' }));

    const listbox = screen.getByRole('listbox', { name: 'Sort options' });
    expect(onOpenChange).toHaveBeenCalledWith(true);
    expect(within(listbox).getByRole('option', { name: 'Name' }).getAttribute('aria-selected')).toBe('true');

    await fireEvent.click(within(listbox).getByRole('option', { name: 'Score' }));

    expect(onSelect).toHaveBeenCalledWith('score');
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole('listbox', { name: 'Sort options' })).toBeNull();
  });

  it('does not select disabled options', async () => {
    const onOpenChange = vi.fn();
    const onSelect = vi.fn();

    render(ListboxPopover, {
      triggerLabel: 'Filter students',
      ariaLabel: 'Filter options',
      value: 'name',
      options,
      onOpenChange,
      onSelect
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students' }));
    const disabledOption = screen.getByRole('option', { name: 'Status' });
    expect(disabledOption.hasAttribute('disabled')).toBe(true);

    await fireEvent.click(disabledOption);

    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByRole('listbox', { name: 'Filter options' })).toBeTruthy();
  });

  it('propagates close requests from the underlying popover', async () => {
    const onOpenChange = vi.fn();

    render(ListboxPopover, {
      open: true,
      triggerLabel: 'Display',
      ariaLabel: 'Display options',
      value: 'name',
      options,
      onOpenChange
    });

    expect(screen.getByRole('listbox', { name: 'Display options' })).toBeTruthy();

    await fireEvent.keyDown(document, { key: 'Escape' });

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(screen.queryByRole('listbox', { name: 'Display options' })).toBeNull();
  });
});
