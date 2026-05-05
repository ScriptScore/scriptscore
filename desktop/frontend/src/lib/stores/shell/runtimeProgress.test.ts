// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import { createRuntimeProgressTracker } from './runtimeProgress';
import type { RuntimeJobEvent } from '$lib/types';

function runtimeEvent(
  eventType: RuntimeJobEvent['eventType'],
  payload: Record<string, unknown> = {},
  commandName = 'smoke.ping'
): RuntimeJobEvent {
  return {
    eventType,
    commandName,
    workerStatus: eventType === 'job_failed' ? 'error' : 'busy',
    requestId: 'req_1',
    jobId: 'job_1',
    payload
  };
}

describe('runtimeProgress', () => {
  it('tracks queued and running pipeline progress', () => {
    const tracker = createRuntimeProgressTracker();

    expect(tracker.handleEvent(runtimeEvent('job_queued', { queuePosition: 2 }))).toBeNull();
    expect(tracker.handleEvent(runtimeEvent('job_started', { schedulerActiveJobs: 1, schedulerPendingJobs: 1 }))).toBe(0);
    expect(
      tracker.handleEvent(
        runtimeEvent('job_progress', {
          schedulerActiveJobs: 1,
          schedulerPendingJobs: 1,
          event: 'started',
          data: { total_stages: 2 },
          progress: { percent: 50 }
        })
      )
    ).toBe(13);
    expect(
      tracker.handleEvent(
        runtimeEvent('job_finished', { schedulerActiveJobs: 1, schedulerPendingJobs: 0 })
      )
    ).toBe(50);
  });

  it('maps intake redaction and ingest progress into the staged 0-100 range', () => {
    const tracker = createRuntimeProgressTracker();

    expect(
      tracker.handleEvent(
        runtimeEvent(
          'job_started',
          {
            schedulerActiveJobs: 1,
            schedulerPendingJobs: 0,
            intakePipelineActive: true,
            intakePipelineRedactTotal: 2,
            intakePipelineRedactIndex: 1
          },
          'scans.pdf-create-redacted'
        )
      )
    ).toBeNull();
    expect(
      tracker.handleEvent(
        runtimeEvent(
          'job_progress',
          {
            schedulerActiveJobs: 1,
            schedulerPendingJobs: 0,
            intakePipelineActive: true,
            intakePipelineRedactTotal: 2,
            intakePipelineRedactIndex: 1,
            event: 'started',
            data: { total_stages: 1 },
            progress: { percent: 100 }
          },
          'scans.pdf-create-redacted'
        )
      )
    ).toBe(50);

    expect(
      tracker.handleEvent(
        runtimeEvent(
          'job_started',
          {
            schedulerActiveJobs: 1,
            schedulerPendingJobs: 0,
            intakePipelineActive: true,
            intakePipelineRedactTotal: 0,
            intakePipelineRedactIndex: 0
          },
          'scans.ingest'
        )
      )
    ).toBe(50);
    expect(
      tracker.handleEvent(
        runtimeEvent(
          'job_progress',
          {
            schedulerActiveJobs: 1,
            schedulerPendingJobs: 0,
            intakePipelineActive: true,
            intakePipelineRedactTotal: 0,
            intakePipelineRedactIndex: 0,
            progress: { percent: 100 }
          },
          'scans.ingest'
        )
      )
    ).toBe(100);
  });

  it('ignores background alignment-mark detection progress', () => {
    const tracker = createRuntimeProgressTracker();

    expect(tracker.handleEvent(runtimeEvent('job_started', {}, 'exam.setup'))).toBe(0);
    expect(
      tracker.handleEvent(
        runtimeEvent(
          'job_progress',
          {
            progress: { percent: 60 }
          },
          'exam.setup'
        )
      )
    ).toBe(60);
    expect(tracker.handleEvent(runtimeEvent('job_finished', {}, 'exam.setup'))).toBeNull();

    expect(tracker.handleEvent(runtimeEvent('job_started', {}, 'scans.pdf-detect-aruco'))).toBeNull();
    expect(
      tracker.handleEvent(
        runtimeEvent(
          'job_progress',
          {
            progress: { percent: 5 }
          },
          'scans.pdf-detect-aruco'
        )
      )
    ).toBeNull();
    expect(
      tracker.handleEvent(runtimeEvent('job_finished', {}, 'scans.pdf-detect-aruco'))
    ).toBeNull();
  });

  it('resets on failed or cancelled jobs', () => {
    const tracker = createRuntimeProgressTracker();

    tracker.handleEvent(runtimeEvent('job_started', { schedulerActiveJobs: 1, schedulerPendingJobs: 0 }));
    expect(tracker.getProgress()).toBe(0);
    expect(tracker.handleEvent(runtimeEvent('job_failed'))).toBeNull();
    expect(tracker.getProgress()).toBeNull();

    tracker.handleEvent(runtimeEvent('job_started', { schedulerActiveJobs: 1, schedulerPendingJobs: 0 }));
    expect(tracker.handleEvent(runtimeEvent('job_cancelled'))).toBeNull();
    expect(tracker.getProgress()).toBeNull();
  });
});
