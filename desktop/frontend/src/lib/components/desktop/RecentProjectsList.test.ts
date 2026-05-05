// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import RecentProjectsList from './RecentProjectsList.svelte';

describe('RecentProjectsList', () => {
  it('opens a selected recent project path', async () => {
    const onOpenRecentProject = vi.fn();

    render(RecentProjectsList, {
      recentProjects: [
        {
          displayName: 'Midterm',
          courseCode: 'BIO 101',
          projectPath: '/tmp/midterm',
          openedAt: '2026-04-27T00:00:00Z'
        }
      ],
      onOpenRecentProject
    });

    await fireEvent.click(screen.getByRole('button', { name: /Midterm/ }));

    expect(onOpenRecentProject).toHaveBeenCalledWith('/tmp/midterm');
  });
});
