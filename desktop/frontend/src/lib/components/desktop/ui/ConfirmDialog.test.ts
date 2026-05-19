// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ConfirmDialog from './ConfirmDialog.svelte';

describe('ConfirmDialog', () => {
  it('routes cancel and confirm callbacks', async () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(ConfirmDialog, {
      open: true,
      title: 'Delete submission?',
      description: 'This cannot be undone.',
      confirmLabel: 'Delete',
      onCancel,
      onConfirm
    });

    expect(screen.getByRole('dialog', { name: 'Delete submission?' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('can render as a one-button acknowledgement dialog', () => {
    render(ConfirmDialog, {
      open: true,
      title: 'No updates',
      description: 'You are on the current release. No updates are available.',
      confirmLabel: 'OK',
      cancelLabel: null
    });

    expect(screen.getByRole('dialog', { name: 'No updates' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'OK' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Cancel' })).toBeNull();
  });
});
