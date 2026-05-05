// SPDX-License-Identifier: AGPL-3.0-only
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const desktopMocks = vi.hoisted(() => ({
  getShellState: vi.fn(),
  isDesktopHost: vi.fn(),
  listenRuntimeJobEvents: vi.fn()
}));

vi.mock('$lib/desktop', () => desktopMocks);

import {
  ensureRuntimeJobBridge,
  refreshShellState,
  shellState,
  teardownRuntimeJobBridge
} from './shell';

const initialShellState = {
  currentProject: null,
  workerStatus: 'starting' as const,
  workerActivity: { activeJobs: [], pendingJobCount: 0 },
  lastRuntimeError: null,
  debugFeatures: { redactionToggle: false }
};

describe('shell store', () => {
  beforeEach(() => {
    desktopMocks.getShellState.mockReset();
    desktopMocks.isDesktopHost.mockReset();
    desktopMocks.listenRuntimeJobEvents.mockReset();
    shellState.set(initialShellState);
    teardownRuntimeJobBridge();
  });

  it('refreshes shell state from the desktop API', async () => {
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: {
        projectId: 'proj_1',
        displayName: 'Midterm 1',
        subject: 'Physics',
        courseCode: 'PHYS 221',
        lmsCourseId: null,
        projectPath: '/tmp/project',
        createdAt: '1',
        updatedAt: '1'
      },
      workerStatus: 'ready',
      workerActivity: { activeJobs: [], pendingJobCount: 0 },
      lastRuntimeError: null
    });

    await refreshShellState();

    expect(get(shellState).currentProject?.displayName).toBe('Midterm 1');
    expect(get(shellState).workerStatus).toBe('ready');
  });

  it('does not register the runtime bridge in browser preview mode', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(false);

    await ensureRuntimeJobBridge();

    expect(desktopMocks.listenRuntimeJobEvents).not.toHaveBeenCalled();
  });

  it('registers the runtime bridge once and applies runtime error events', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    const unlisten = vi.fn();
    let runtimeHandler: ((event: Record<string, unknown>) => void) | null = null;
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: Record<string, unknown>) => void;
      return unlisten;
    });

    await ensureRuntimeJobBridge();
    await ensureRuntimeJobBridge();

    expect(desktopMocks.listenRuntimeJobEvents).toHaveBeenCalledTimes(1);

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    (runtimeHandler as (event: Record<string, unknown>) => void)({
      eventType: 'runtime_error',
      commandName: 'smoke.ping',
      workerStatus: 'error',
      requestId: null,
      jobId: null,
      payload: { message: 'worker unavailable' }
    });

    expect(get(shellState)).toEqual({
      currentProject: null,
      workerStatus: 'error',
      workerActivity: { activeJobs: [], pendingJobCount: 0 },
      debugFeatures: { redactionToggle: false },
      lastRuntimeError: 'worker unavailable'
    });

    teardownRuntimeJobBridge();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('surfaces job_failed terminal errors in lastRuntimeError (nested CLI error message)', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    const unlisten = vi.fn();
    let runtimeHandler: ((event: Record<string, unknown>) => void) | null = null;
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: Record<string, unknown>) => void;
      return unlisten;
    });

    await ensureRuntimeJobBridge();

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    (runtimeHandler as (event: Record<string, unknown>) => void)({
      eventType: 'job_failed',
      commandName: 'exam.analyze',
      workerStatus: 'ready',
      requestId: 'req-1',
      jobId: 'job-1',
      payload: {
        error: {
          message: 'Ollama is unreachable.',
          code: 'ollama_unreachable'
        },
        schedulerActiveJobs: 0,
        schedulerPendingJobs: 0
      }
    });

    expect(get(shellState).lastRuntimeError).toBe('exam.analyze: Ollama is unreachable.');
    teardownRuntimeJobBridge();
  });

  it('applies worker activity snapshots from runtime events', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    let runtimeHandler: ((event: Record<string, unknown>) => void) | null = null;
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: Record<string, unknown>) => void;
      return vi.fn();
    });

    await ensureRuntimeJobBridge();

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    (runtimeHandler as (event: Record<string, unknown>) => void)({
      eventType: 'job_progress',
      commandName: 'grading.score-preliminary',
      workerStatus: 'busy',
      requestId: 'req-1',
      jobId: 'job-1',
      payload: {
        schedulerActiveJobDetails: [
          {
            jobId: 'job-1',
            commandName: 'grading.score-preliminary',
            startedAt: '2026-04-02T00:00:01Z'
          }
        ],
        schedulerPendingJobs: 2
      }
    });

    expect(get(shellState).workerActivity).toEqual({
      activeJobs: [
        {
          jobId: 'job-1',
          commandName: 'grading.score-preliminary',
          startedAt: '2026-04-02T00:00:01Z'
        }
      ],
      pendingJobCount: 2
    });
  });
});
