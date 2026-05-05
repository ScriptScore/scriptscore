// SPDX-License-Identifier: AGPL-3.0-only
import { render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import NotificationToasts from './NotificationToasts.svelte';
import { notifications } from '$lib/stores/notifications';

describe('NotificationToasts', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    notifications.clear();
  });

  afterEach(() => {
    notifications.clear();
    vi.useRealTimers();
  });

  it('uses status roles for success and info toasts as the queue advances', async () => {
    render(NotificationToasts);

    notifications.pushSuccess('Saved', 1000);
    notifications.pushInfo('Queued');
    await tick();

    expect(screen.getAllByRole('status')).toHaveLength(1);
    expect(screen.getByText('Saved')).toBeTruthy();

    vi.advanceTimersByTime(1000);
    await tick();
    vi.advanceTimersByTime(480);
    await tick();

    expect(screen.getByText('Queued')).toBeTruthy();
  });

  it('uses alert roles for warning and error toasts as the queue advances', async () => {
    render(NotificationToasts);

    notifications.pushWarning('Missing assignment', 1000);
    notifications.pushError('Upload failed');
    await tick();

    expect(screen.getAllByRole('alert')).toHaveLength(1);
    expect(screen.getByText('Missing assignment')).toBeTruthy();

    vi.advanceTimersByTime(1000);
    await tick();
    vi.advanceTimersByTime(480);
    await tick();

    expect(screen.getByText('Upload failed')).toBeTruthy();
  });
});
