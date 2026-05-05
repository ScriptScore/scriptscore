// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import {
  isWorkflowRuntimeCommand,
  shouldEnsureAutomaticRubricsAfterTerminalJob,
  shouldRefreshWorkspaceAfterTerminalJob,
  shouldRefreshWorkspaceDuringRuntimeEvent
} from './workspaceRefreshPolicy';
import type { RuntimeJobEvent } from '$lib/types';

function runtimeEvent(
  eventType: RuntimeJobEvent['eventType'],
  commandName: string,
  payload: Record<string, unknown> = {}
): RuntimeJobEvent {
  return {
    eventType,
    commandName,
    workerStatus: 'busy',
    requestId: 'req_1',
    jobId: 'job_1',
    payload
  };
}

describe('workspaceRefreshPolicy', () => {
  it('recognizes workflow runtime commands', () => {
    expect(isWorkflowRuntimeCommand('scans.parse')).toBe(true);
    expect(isWorkflowRuntimeCommand('grading.score-preliminary')).toBe(true);
    expect(isWorkflowRuntimeCommand('confirm_student_alignment')).toBe(true);
    expect(isWorkflowRuntimeCommand('results.export')).toBe(false);
    expect(isWorkflowRuntimeCommand('exam.analyze')).toBe(false);
  });

  it('refreshes after terminal analysis and workflow jobs', () => {
    expect(
      shouldRefreshWorkspaceAfterTerminalJob(runtimeEvent('job_finished', 'exam.analyze'))
    ).toBe(true);
    expect(
      shouldRefreshWorkspaceAfterTerminalJob(runtimeEvent('job_failed', 'scans.parse'))
    ).toBe(true);
    expect(
      shouldRefreshWorkspaceAfterTerminalJob(runtimeEvent('job_finished', 'smoke.ping'))
    ).toBe(false);
  });

  it('keeps automatic rubric enqueue policy outside the route', () => {
    expect(
      shouldEnsureAutomaticRubricsAfterTerminalJob(runtimeEvent('job_finished', 'exam.analyze'))
    ).toBe(true);
    expect(
      shouldEnsureAutomaticRubricsAfterTerminalJob(runtimeEvent('job_failed', 'exam.analyze'))
    ).toBe(false);
    expect(
      shouldEnsureAutomaticRubricsAfterTerminalJob(
        runtimeEvent('job_finished', 'exam.generate-rubric')
      )
    ).toBe(false);
  });

  it('refreshes during workflow runtime events when the state changed or workflow work started', () => {
    expect(
      shouldRefreshWorkspaceDuringRuntimeEvent(
        runtimeEvent('job_started', 'scans.parse')
      )
    ).toBe(true);
    expect(
      shouldRefreshWorkspaceDuringRuntimeEvent(
        runtimeEvent('job_progress', 'scans.parse', { workflowStateUpdated: true })
      )
    ).toBe(true);
    expect(
      shouldRefreshWorkspaceDuringRuntimeEvent(runtimeEvent('job_progress', 'smoke.ping'))
    ).toBe(false);
  });
});
