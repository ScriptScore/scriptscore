// SPDX-License-Identifier: AGPL-3.0-only
import type { RuntimeJobEvent } from '$lib/types';

type JobFinishedHandler = (event: RuntimeJobEvent) => void;
type JobFailedHandler = (event: RuntimeJobEvent) => void;
type RuntimeJobEventHandler = (event: RuntimeJobEvent) => void;

interface JobHandlerEntry {
  jobId: string | null;
  onFinished: JobFinishedHandler;
  onFailed: JobFailedHandler;
}

function matchesJobFilter(entry: JobHandlerEntry, eventJobId: string | null): boolean {
  if (entry.jobId === null) {
    return true;
  }
  return entry.jobId === eventJobId;
}

export interface CompletionRegistry {
  onJobFinished(handler: JobFinishedHandler, jobId?: string | null): () => void;
  onJobFailed(handler: JobFailedHandler, jobId?: string | null): () => void;
  onJobCompletion(
    handlers: { onFinished: JobFinishedHandler; onFailed: JobFailedHandler },
    jobId?: string | null
  ): () => void;
  onRuntimeJobEvent(handler: RuntimeJobEventHandler): () => void;
  emitRuntimeEvent(event: RuntimeJobEvent): void;
  emitTerminalEvent(event: RuntimeJobEvent): void;
  clear(): void;
}

export function createCompletionRegistry(): CompletionRegistry {
  const jobHandlers = new Set<JobHandlerEntry>();
  const completedJobEvents = new Map<string, RuntimeJobEvent>();
  const runtimeEventHandlers = new Set<RuntimeJobEventHandler>();

  function registerJobHandler(entry: JobHandlerEntry): () => void {
    jobHandlers.add(entry);
    if (entry.jobId !== null) {
      const completedEvent = completedJobEvents.get(entry.jobId);
      if (completedEvent) {
        queueMicrotask(() => {
          if (!jobHandlers.has(entry)) {
            return;
          }
          if (completedEvent.eventType === 'job_finished') {
            entry.onFinished(completedEvent);
          } else if (
            completedEvent.eventType === 'job_failed' ||
            completedEvent.eventType === 'job_cancelled' ||
            completedEvent.eventType === 'job_skipped'
          ) {
            entry.onFailed(completedEvent);
          }
        });
      }
    }
    return () => {
      jobHandlers.delete(entry);
    };
  }

  function notifyJobHandlers(
    event: RuntimeJobEvent,
    notify: (entry: JobHandlerEntry) => void
  ): void {
    for (const entry of jobHandlers) {
      if (matchesJobFilter(entry, event.jobId)) {
        notify(entry);
      }
    }
  }

  return {
    onJobFinished(handler, jobId) {
      return registerJobHandler({
        jobId: jobId ?? null,
        onFinished: handler,
        onFailed: () => {}
      });
    },
    onJobFailed(handler, jobId) {
      return registerJobHandler({
        jobId: jobId ?? null,
        onFinished: () => {},
        onFailed: handler
      });
    },
    onJobCompletion(handlers, jobId) {
      return registerJobHandler({
        jobId: jobId ?? null,
        onFinished: handlers.onFinished,
        onFailed: handlers.onFailed
      });
    },
    onRuntimeJobEvent(handler) {
      runtimeEventHandlers.add(handler);
      return () => {
        runtimeEventHandlers.delete(handler);
      };
    },
    emitRuntimeEvent(event) {
      for (const handler of runtimeEventHandlers) {
        handler(event);
      }
    },
    emitTerminalEvent(event) {
      if (event.jobId) {
        completedJobEvents.set(event.jobId, event);
      }
      if (event.eventType === 'job_finished') {
        notifyJobHandlers(event, (entry) => {
          entry.onFinished(event);
        });
        return;
      }
      notifyJobHandlers(event, (entry) => {
        entry.onFailed(event);
      });
    },
    clear() {
      jobHandlers.clear();
      completedJobEvents.clear();
      runtimeEventHandlers.clear();
    }
  };
}
