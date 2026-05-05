// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ProjectTopBar from './ProjectTopBar.svelte';

describe('ProjectTopBar', () => {
  it('opens a compact worker popover with active job and queue details', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-02T00:05:00Z'));
    const onOpenActiveTraces = vi.fn();
    try {
      render(ProjectTopBar, {
        appTitle: 'ScriptScore Desktop',
        workerStatus: 'busy',
        workerProgress: 42,
        workerActivity: {
          activeJobs: [
            {
              jobId: 'job-active-1',
              commandName: 'grading.score-preliminary',
              startedAt: '2026-04-02T00:00:01Z'
            }
          ],
          pendingJobCount: 3
        },
        workflowStage: 'student_workflow_running',
        workflowLabel: 'Students running',
        onOpenActiveTraces
      });

      const workerButton = screen.getByRole('button', { name: 'Busy 42%' });
      expect(workerButton.getAttribute('title')).toBe(
        'Worker status. Click for activity and queue details.'
      );

      await fireEvent.click(workerButton);

      const dialog = screen.getByRole('dialog', { name: 'Worker activity' });
      expect(dialog).toBeTruthy();
      expect(within(dialog).queryByText('Worker')).toBeNull();
      expect(within(dialog).queryByText('Busy 42%')).toBeNull();
      expect(screen.getByText('Grade answers')).toBeTruthy();
      expect(screen.queryByText('job-active-1')).toBeNull();
      expect(screen.getByText('Started')).toBeTruthy();
      expect(screen.getByText('Duration')).toBeTruthy();
      expect(screen.getByText('4m 59s')).toBeTruthy();
      expect(screen.getByText('3 queued')).toBeTruthy();

      await fireEvent.click(screen.getByRole('button', { name: 'Show More' }));

      expect(onOpenActiveTraces).toHaveBeenCalledWith(['job-active-1']);
    } finally {
      vi.useRealTimers();
    }
  });

  it('opens unfiltered trace history when no job is active', async () => {
    const onOpenActiveTraces = vi.fn();
    render(ProjectTopBar, {
      workerStatus: 'ready',
      workerActivity: { activeJobs: [], pendingJobCount: 0 },
      onOpenActiveTraces
    });

    await fireEvent.click(screen.getByRole('button', { name: 'ready' }));

    expect(screen.getByText('No active job')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Show More' }));

    expect(onOpenActiveTraces).toHaveBeenCalledWith([]);
  });
});
