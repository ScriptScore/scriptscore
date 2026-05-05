// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import {
  hostWorkflowData,
  hostWorkflowErrorMessage,
  hostWorkflowChangedWorkspace
} from './hostWorkflowCompletion';
import type { RuntimeJobEvent } from '$lib/types';

function event(payload: Record<string, unknown>): RuntimeJobEvent {
  return {
    eventType: 'job_finished',
    commandName: 'export_stamped_template_pdf',
    workerStatus: 'ready',
    requestId: 'req_1',
    jobId: 'job_1',
    payload
  };
}

describe('hostWorkflowCompletion', () => {
  it('returns data only for the declared result kind', () => {
    expect(
      hostWorkflowData<{ ok: boolean }>(
        event({ resultKind: 'workspace', data: { ok: true }, workspaceChanged: true }),
        'workspace'
      )
    ).toEqual({ ok: true });
    expect(() => hostWorkflowData(event({ resultKind: 'shell', data: {} }), 'workspace')).toThrow(
      /expected workspace/
    );
  });

  it('extracts standardized error messages and workspace-changed signals', () => {
    const failed = event({
      resultKind: 'error',
      workspaceChanged: true,
      error: { message: 'Export failed.' }
    });
    expect(hostWorkflowErrorMessage(failed)).toBe('Export failed.');
    expect(hostWorkflowChangedWorkspace(failed)).toBe(true);
  });
});

