// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import RecentProjectsList from './RecentProjectsList.svelte';

describe('RecentProjectsList', () => {
  it('opens a selected recent project path', async () => {
    const onOpenRecentProject = vi.fn();
    const projectPath =
      '/Users/leo/Documents/ScriptScore Development/scriptscore/examples/very/deep/folder/midterm';

    render(RecentProjectsList, {
      recentProjects: [
        {
          displayName: 'Midterm',
          courseCode: 'BIO 101',
          projectPath,
          openedAt: '2026-04-27T00:00:00Z'
        }
      ],
      onOpenRecentProject
    });

    const card = screen.getByRole('button', { name: /Midterm/ });
    const pathLabel = screen.getByLabelText(projectPath);

    expect(card.title).toBe(projectPath);
    expect(card.className).toContain('min-w-0');
    expect(pathLabel.className).toContain('justify-end');
    expect(screen.getByText('/Users/leo/Documents/ScriptScore Development/scriptscore/examples/very/deep/folder/').className).toContain('[direction:rtl]');
    expect(screen.getByText('midterm').className).toContain('shrink-0');

    await fireEvent.click(card);

    expect(onOpenRecentProject).toHaveBeenCalledWith(projectPath);
  });
});
