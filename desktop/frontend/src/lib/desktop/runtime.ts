// SPDX-License-Identifier: AGPL-3.0-only
import type { JobTraceState, JobTraceSummary, RuntimeJobEvent, ShellState } from '$lib/types';

import {
  invokeDesktopHost,
  listenRuntimeJobEventsInternal
} from './shared';

export function startJob(
  commandName: string,
  requestPayload: Record<string, unknown>
): Promise<string> {
  return invokeDesktopHost<string>('start_job', {
    commandName,
    requestPayload
  });
}

export function cancelActiveJob(jobId?: string | null): Promise<ShellState> {
  return invokeDesktopHost<ShellState>('cancel_active_job', {
    jobId: jobId ?? null
  });
}

export function getJobTrace(
  jobId?: string | null,
  commandName?: string | null
): Promise<JobTraceState | null> {
  return invokeDesktopHost<JobTraceState | null>('get_job_trace', {
    jobId: jobId ?? null,
    commandName: commandName ?? null
  });
}

export function listJobTraces(): Promise<JobTraceSummary[]> {
  return invokeDesktopHost<JobTraceSummary[]>('list_job_traces');
}

export function listenRuntimeJobEvents(
  onEvent: (event: RuntimeJobEvent) => void
) {
  return listenRuntimeJobEventsInternal(onEvent);
}
