// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { JobTraceState, JobTraceSummary } from '$lib/types';
import { buildJobTrace } from '../../../test/traceTestUtils';
import TraceHistoryDialog from './TraceHistoryDialog.svelte';

function summary(overrides: Partial<JobTraceSummary> = {}): JobTraceSummary {
  return {
    jobId: 'job_1',
    commandName: 'exam.setup',
    state: 'succeeded',
    submittedAt: '2026-04-01T00:00:00Z',
    startedAt: '2026-04-01T00:00:01Z',
    finishedAt: '2026-04-01T00:00:04Z',
    eventCount: 2,
    studentRefs: [],
    ...overrides
  };
}

describe('TraceHistoryDialog', () => {
  it('opens, closes, and shows an empty trace history state', async () => {
    const onClose = vi.fn();
    render(TraceHistoryDialog, {
      open: true,
      loadSummaries: vi.fn().mockResolvedValue([]),
      loadTrace: vi.fn(),
      onClose
    });

    expect(screen.getByRole('dialog', { name: 'Trace history' })).toBeTruthy();
    expect(await screen.findByText('No project traces are available yet.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('lists summaries, selects a trace, and renders detail tabs', async () => {
    const traces = [
      summary({
        jobId: 'job_setup',
        commandName: 'exam.setup',
        state: 'succeeded',
        submittedAt: '2026-04-03T00:00:00Z'
      }),
      summary({
        jobId: 'job_smoke',
        commandName: 'smoke.ping',
        state: 'failed',
        submittedAt: '2026-04-02T00:00:00Z'
      })
    ];
    const loadSummaries = vi.fn().mockResolvedValue(traces);
    const loadTrace = vi.fn().mockImplementation((jobId: string) =>
      Promise.resolve(
        buildJobTrace({
          jobId,
          commandName: jobId === 'job_smoke' ? 'smoke.ping' : 'exam.setup',
          request: { selected: jobId },
          result: { ok: jobId !== 'job_smoke' }
        })
      )
    );

    render(TraceHistoryDialog, {
      open: true,
      loadSummaries,
      loadTrace
    });

    expect(await screen.findByRole('button', { name: /exam\.setup/ })).toBeTruthy();
    await waitFor(() => expect(loadTrace).toHaveBeenCalledWith('job_setup'));

    await fireEvent.click(screen.getByRole('button', { name: /smoke\.ping/ }));
    await waitFor(() => expect(loadTrace).toHaveBeenCalledWith('job_smoke'));

    await fireEvent.click(screen.getByRole('tab', { name: 'Request' }));
    expect(await screen.findByText(/"selected": "job_smoke"/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('tab', { name: 'Response' }));
    expect(screen.getByText(/"ok": false/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('tab', { name: 'Event Stream' }));
    expect(screen.getByText(/"eventType": "started"/)).toBeTruthy();
  });

  it('filters rows by search, command, and state', async () => {
    render(TraceHistoryDialog, {
      open: true,
      loadSummaries: vi.fn().mockResolvedValue([
        summary({ jobId: 'job_setup', commandName: 'exam.setup', state: 'succeeded' }),
        summary({ jobId: 'job_analyze', commandName: 'exam.analyze', state: 'failed' }),
        summary({ jobId: 'job_rubric', commandName: 'exam.generate-rubric', state: 'queued' })
      ]),
      loadTrace: vi.fn().mockResolvedValue(buildJobTrace())
    });

    expect(await screen.findByRole('button', { name: /exam\.setup/ })).toBeTruthy();

    await fireEvent.input(screen.getByPlaceholderText('Search job, command, state, student, or time'), {
      target: { value: 'rubric' }
    });
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /exam\.generate-rubric/ })).toBeTruthy();
      expect(screen.queryByRole('button', { name: /exam\.setup/ })).toBeNull();
    });

    await fireEvent.input(screen.getByPlaceholderText('Search job, command, state, student, or time'), {
      target: { value: '' }
    });
    await fireEvent.click(screen.getByRole('combobox', { name: 'Command filter' }));
    await fireEvent.click(await screen.findByRole('option', { name: 'exam.analyze' }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /exam\.analyze/ })).toBeTruthy();
      expect(screen.queryByRole('button', { name: /exam\.setup/ })).toBeNull();
    });

    await fireEvent.click(screen.getByRole('combobox', { name: 'State filter' }));
    await fireEvent.click(await screen.findByRole('option', { name: 'succeeded' }));
    expect(screen.getByText('No traces match the current search and filters.')).toBeTruthy();

    const dialog = screen.getByRole('dialog', { name: 'Trace history' });
    expect(within(dialog).getByRole('button', { name: 'Refresh' })).toBeTruthy();
  });

  it('does not render stale trace details after filters clear the selected row', async () => {
    let resolveTrace: (trace: JobTraceState) => void = () => {};
    const pendingTrace = new Promise<JobTraceState>((resolve) => {
      resolveTrace = resolve;
    });

    const loadTrace = vi.fn().mockImplementation((jobId: string) => {
      if (jobId === 'job_pending') {
        return pendingTrace;
      }
      return Promise.resolve(buildJobTrace({ jobId }));
    });

    render(TraceHistoryDialog, {
      open: true,
      loadSummaries: vi.fn().mockResolvedValue([
        summary({
          jobId: 'job_pending',
          commandName: 'exam.pending',
          state: 'running',
          submittedAt: '2026-04-01T00:00:00Z'
        }),
        summary({
          jobId: 'job_setup',
          commandName: 'exam.setup',
          state: 'succeeded',
          submittedAt: '2026-04-02T00:00:00Z'
        })
      ]),
      loadTrace
    });

    expect(await screen.findByRole('button', { name: /exam\.setup/ })).toBeTruthy();
    await waitFor(() => expect(loadTrace).toHaveBeenCalledWith('job_setup'));
    await fireEvent.click(screen.getByRole('button', { name: /exam\.pending/ }));
    await waitFor(() => expect(loadTrace).toHaveBeenCalledWith('job_pending'));

    await fireEvent.input(screen.getByPlaceholderText('Search job, command, state, student, or time'), {
      target: { value: 'no matching trace' }
    });
    expect(screen.getByText('No traces match the current search and filters.')).toBeTruthy();

    resolveTrace(buildJobTrace({ jobId: 'job_setup', request: { stale: true } }));

    await waitFor(() => {
      expect(screen.queryByText('Trace Details')).toBeNull();
      expect(
        screen.getByText('Select a trace to inspect request, response, and event details.')
      ).toBeTruthy();
    });
  });

  it('formats epoch timestamps and sorts by selectable fields', async () => {
    render(TraceHistoryDialog, {
      open: true,
      loadSummaries: vi.fn().mockResolvedValue([
        summary({
          jobId: 'job_beta',
          commandName: 'zz.beta',
          state: 'queued',
          submittedAt: '1777595217',
          startedAt: '1777595217',
          finishedAt: '1777595227'
        }),
        summary({
          jobId: 'job_alpha',
          commandName: 'aa.alpha',
          state: 'succeeded',
          submittedAt: '1777595200',
          startedAt: '1777595200',
          finishedAt: '1777595202'
        })
      ]),
      loadTrace: vi.fn().mockResolvedValue(buildJobTrace())
    });

    expect(await screen.findByRole('button', { name: /zz\.beta/ })).toBeTruthy();
    expect(screen.queryByText('1777595217')).toBeNull();
    expect(screen.getByRole('button', { name: 'Sort ascending' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Sort traces' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Command' }));

    const rowsByCommand = screen.getAllByRole('button', { name: /aa\.alpha|zz\.beta/ });
    expect(rowsByCommand[0].textContent).toContain('zz.beta');

    await fireEvent.click(screen.getByRole('button', { name: 'Sort ascending' }));
    expect(screen.getByRole('button', { name: 'Sort descending' })).toBeTruthy();

    const rowsAscending = screen.getAllByRole('button', { name: /aa\.alpha|zz\.beta/ });
    expect(rowsAscending[0].textContent).toContain('aa.alpha');
  });

  it('prefilters to active job ids and can clear that filter', async () => {
    render(TraceHistoryDialog, {
      open: true,
      initialJobIds: ['job_active'],
      initialStateFilter: 'running',
      loadSummaries: vi.fn().mockResolvedValue([
        summary({ jobId: 'job_active', commandName: 'scans.parse', state: 'running' }),
        summary({ jobId: 'job_done', commandName: 'exam.setup', state: 'succeeded' })
      ]),
      loadTrace: vi.fn().mockResolvedValue(buildJobTrace())
    });

    expect(await screen.findByRole('button', { name: /scans\.parse/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /exam\.setup/ })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Clear' }));

    expect(await screen.findByRole('button', { name: /exam\.setup/ })).toBeTruthy();
  });

  it('shows and searches student references', async () => {
    render(TraceHistoryDialog, {
      open: true,
      loadSummaries: vi.fn().mockResolvedValue([
        summary({
          jobId: 'job_student_1',
          commandName: 'grading.score-preliminary',
          studentRefs: ['student_abc']
        }),
        summary({ jobId: 'job_other', commandName: 'exam.setup', studentRefs: ['student_xyz'] })
      ]),
      loadTrace: vi.fn().mockResolvedValue(buildJobTrace())
    });

    expect(await screen.findByText('Students: student_abc')).toBeTruthy();

    await fireEvent.input(screen.getByPlaceholderText('Search job, command, state, student, or time'), {
      target: { value: 'student_xyz' }
    });

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /exam\.setup/ })).toBeTruthy();
      expect(screen.queryByRole('button', { name: /grading\.score-preliminary/ })).toBeNull();
    });
  });
});
