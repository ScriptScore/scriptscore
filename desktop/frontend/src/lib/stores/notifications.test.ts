// SPDX-License-Identifier: AGPL-3.0-only
import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { notifications } from './notifications';

describe('notifications store', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    notifications.clear();
  });

  afterEach(() => {
    notifications.clear();
    vi.useRealTimers();
  });

  it('auto-dismisses a success toast after the default timeout', () => {
    notifications.pushSuccess('Settings saved');

    expect(get(notifications)).toHaveLength(1);
    expect(get(notifications)[0]?.message).toBe('Settings saved');

    vi.advanceTimersByTime(2500);

    expect(get(notifications)).toHaveLength(0);
  });

  it('shows one toast and queues later toasts until the ticker advances', () => {
    notifications.pushSuccess('Settings saved');
    notifications.pushInfo('Upload started');
    notifications.pushWarning('Assignment missing');
    notifications.pushError('Upload failed');

    expect(get(notifications)).toEqual([
      expect.objectContaining({
        kind: 'success',
        message: 'Settings saved'
      })
    ]);

    vi.advanceTimersByTime(2500);
    expect(get(notifications)[0]).toEqual(
      expect.objectContaining({
        kind: 'info',
        message: 'Upload started'
      })
    );

    vi.advanceTimersByTime(2500);
    expect(get(notifications)[0]).toEqual(
      expect.objectContaining({
        kind: 'warning',
        message: 'Assignment missing'
      })
    );
  });

  it('advances queued toasts when one toast is dismissed manually', () => {
    const firstId = notifications.pushInfo('Upload started', 1000);
    notifications.pushWarning('Assignment missing', 3000);

    notifications.dismiss(firstId);
    expect(get(notifications).map((toast) => toast.message)).toEqual(['Assignment missing']);

    vi.advanceTimersByTime(2999);
    expect(get(notifications)).toHaveLength(1);

    vi.advanceTimersByTime(1);
    expect(get(notifications)).toHaveLength(0);
  });
});
