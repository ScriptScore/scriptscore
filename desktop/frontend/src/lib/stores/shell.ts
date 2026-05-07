// SPDX-License-Identifier: AGPL-3.0-only
import { writable } from 'svelte/store';
import type { UnlistenFn } from '@tauri-apps/api/event';

import { getShellState, isDesktopHost, listenRuntimeJobEvents } from '$lib/desktop';
import type { RuntimeJobEvent, ShellState, WorkerActivity } from '$lib/types';
import { createCompletionRegistry } from './shell/completionRegistry';
import { createRuntimeProgressTracker } from './shell/runtimeProgress';

const initialState: ShellState = {
  currentProject: null,
  workerStatus: 'starting',
  workerActivity: { activeJobs: [], pendingJobCount: 0 },
  lastRuntimeError: null,
  debugFeatures: { redactionToggle: false }
};

export const shellState = writable<ShellState>(initialState);
export const jobProgress = writable<number | null>(null);

let runtimeJobBridge: Promise<void> | null = null;
let runtimeJobUnlisten: UnlistenFn | null = null;
type JobFinishedHandler = (event: RuntimeJobEvent) => void;
type JobFailedHandler = (event: RuntimeJobEvent) => void;
type RuntimeJobEventHandler = (event: RuntimeJobEvent) => void;

const completionRegistry = createCompletionRegistry();
const runtimeProgressTracker = createRuntimeProgressTracker();

export function onJobFinished(
  handler: JobFinishedHandler,
  jobId?: string | null
): () => void {
  return completionRegistry.onJobFinished(handler, jobId);
}

export function onJobFailed(
  handler: JobFailedHandler,
  jobId?: string | null
): () => void {
  return completionRegistry.onJobFailed(handler, jobId);
}

export function onJobCompletion(
  handlers: { onFinished: JobFinishedHandler; onFailed: JobFailedHandler },
  jobId?: string | null
): () => void {
  return completionRegistry.onJobCompletion(handlers, jobId);
}

export function onRuntimeJobEvent(handler: RuntimeJobEventHandler): () => void {
  return completionRegistry.onRuntimeJobEvent(handler);
}

export async function refreshShellState(): Promise<void> {
  shellState.set(await getShellState());
}

export async function ensureRuntimeJobBridge(): Promise<void> {
  if (!isDesktopHost()) {
    return;
  }
  if (runtimeJobUnlisten != null) {
    return;
  }
  if (runtimeJobBridge != null) {
    await runtimeJobBridge;
    return;
  }
  runtimeJobBridge = listenRuntimeJobEvents((event) => {
    applyRuntimeJobEvent(event);
  }).then((unlisten) => {
    runtimeJobUnlisten = unlisten;
  });
  await runtimeJobBridge;
}

export function teardownRuntimeJobBridge(): void {
  runtimeJobUnlisten?.();
  runtimeJobUnlisten = null;
  runtimeJobBridge = null;
  completionRegistry.clear();
  jobProgress.set(runtimeProgressTracker.reset());
}

function terminalFailureMessage(payload: Record<string, unknown>): string | null {
  if (typeof payload.message === 'string' && payload.message.trim().length > 0) {
    return payload.message;
  }
  const err = payload.error;
  if (err && typeof err === 'object' && err !== null) {
    const nested = err as { message?: unknown };
    if (typeof nested.message === 'string' && nested.message.trim().length > 0) {
      return nested.message;
    }
  }
  return null;
}

function isBackgroundProviderProbe(commandName: string | null | undefined): boolean {
  return commandName === 'runtime.list-llm-models' || commandName === 'runtime.validate-llm-model';
}

function currentRuntimeError(current: ShellState, event: RuntimeJobEvent): string | null {
  if (isBackgroundProviderProbe(event.commandName)) {
    return current.lastRuntimeError;
  }
  if (event.eventType === 'runtime_error' && typeof event.payload.message === 'string') {
    return event.payload.message;
  }
  if (
    event.eventType === 'job_failed' ||
    event.eventType === 'job_cancelled' ||
    event.eventType === 'job_skipped'
  ) {
    const detail = terminalFailureMessage(event.payload);
    if (detail) {
      return event.commandName ? `${event.commandName}: ${detail}` : detail;
    }
  }
  if (event.workerStatus === 'error') {
    return current.lastRuntimeError;
  }
  return null;
}

function applyRuntimeJobEvent(event: RuntimeJobEvent): void {
  const workerActivity = workerActivityFromEvent(event);
  shellState.update((current) => ({
    ...current,
    workerStatus: event.workerStatus,
    workerActivity: workerActivity ?? current.workerActivity ?? { activeJobs: [], pendingJobCount: 0 },
    lastRuntimeError: currentRuntimeError(current, event)
  }));

  completionRegistry.emitRuntimeEvent(event);
  jobProgress.set(runtimeProgressTracker.handleEvent(event));

  if (
    event.eventType === 'job_finished' ||
    event.eventType === 'job_failed' ||
    event.eventType === 'job_cancelled' ||
    event.eventType === 'job_skipped'
  ) {
    completionRegistry.emitTerminalEvent(event);
  }
}

function workerActivityFromEvent(event: RuntimeJobEvent): WorkerActivity | null {
  const activeJobs = activeJobsFromPayload(event.payload.schedulerActiveJobDetails);
  const pendingJobCount = numberFromPayload(event.payload.schedulerPendingJobs);
  if (activeJobs === null && pendingJobCount === null) {
    return null;
  }
  return {
    activeJobs: activeJobs ?? [],
    pendingJobCount: pendingJobCount ?? 0
  };
}

function activeJobsFromPayload(value: unknown): WorkerActivity['activeJobs'] | null {
  if (!Array.isArray(value)) {
    return null;
  }
  return value
    .map((item) => {
      if (!item || typeof item !== 'object') {
        return null;
      }
      const entry = item as { jobId?: unknown; commandName?: unknown; startedAt?: unknown };
      if (typeof entry.jobId !== 'string' || typeof entry.commandName !== 'string') {
        return null;
      }
      return {
        jobId: entry.jobId,
        commandName: entry.commandName,
        startedAt: typeof entry.startedAt === 'string' ? entry.startedAt : null
      };
    })
    .filter(
      (item): item is { jobId: string; commandName: string; startedAt: string | null } =>
        item !== null
    );
}

function numberFromPayload(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : null;
}
