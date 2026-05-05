// SPDX-License-Identifier: AGPL-3.0-only
import type { JobTraceState } from '$lib/types';

export function buildJobTrace(overrides: Partial<JobTraceState> = {}): JobTraceState {
  return {
    jobId: 'job_1',
    commandName: 'exam.setup',
    state: 'finished',
    submittedAt: '2026-04-09T00:00:00Z',
    startedAt: '2026-04-09T00:00:01Z',
    finishedAt: '2026-04-09T00:00:05Z',
    request: { input: 'value' },
    result: { ok: true, output: 'done' },
    error: null,
    events: [
      {
        sequence: 1,
        eventType: 'started',
        progress: { completed: 0, total: 2 },
        scope: null,
        data: { step: 'loading' },
        createdAt: '2026-04-09T00:00:01Z'
      },
      {
        sequence: 2,
        eventType: 'step_result',
        progress: null,
        scope: null,
        data: { ok: true, degraded: false },
        createdAt: '2026-04-09T00:00:03Z'
      }
    ],
    ...overrides
  };
}
