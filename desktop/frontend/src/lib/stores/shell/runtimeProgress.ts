// SPDX-License-Identifier: AGPL-3.0-only
import type { RuntimeJobEvent } from '$lib/types';

const hiddenProgressCommands = new Set(['scans.pdf-detect-aruco']);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readSchedulerSnapshot(payload: Record<string, unknown>): { active: number; pending: number } {
  const a = payload.schedulerActiveJobs;
  const p = payload.schedulerPendingJobs;
  return {
    active: typeof a === 'number' ? a : 1,
    pending: typeof p === 'number' ? p : 0
  };
}

function readIntakePipeline(payload: Record<string, unknown>): {
  active: boolean;
  redactTotal: number;
  redactIndex: number;
} | null {
  if (payload.intakePipelineActive !== true) {
    return null;
  }
  return {
    active: true,
    redactTotal:
      typeof payload.intakePipelineRedactTotal === 'number' ? payload.intakePipelineRedactTotal : 0,
    redactIndex:
      typeof payload.intakePipelineRedactIndex === 'number' ? payload.intakePipelineRedactIndex : 0
  };
}

interface ProgressSnapshot {
  totalStages: number;
  currentStage: number;
  pipelineJobsCompleted: number;
  pipelineExtentMax: number;
  progress: number | null;
}

function applyTrackedProgress(snapshot: ProgressSnapshot): void {
  snapshot.currentStage = 1;
  snapshot.totalStages = 1;
  snapshot.pipelineExtentMax = Math.max(snapshot.pipelineExtentMax, 1);
  const total = Math.max(1, snapshot.pipelineExtentMax);
  snapshot.progress =
    snapshot.pipelineJobsCompleted === 0
      ? 0
      : Math.round((snapshot.pipelineJobsCompleted / total) * 100);
}

function resetProgress(snapshot: ProgressSnapshot): void {
  snapshot.totalStages = 1;
  snapshot.currentStage = 1;
  snapshot.pipelineJobsCompleted = 0;
  snapshot.pipelineExtentMax = 0;
  snapshot.progress = null;
}

function extendPipelineExtentFromScheduler(
  snapshot: ProgressSnapshot,
  payload: Record<string, unknown>
): void {
  const { active, pending } = readSchedulerSnapshot(payload);
  const extent = snapshot.pipelineJobsCompleted + active + pending;
  snapshot.pipelineExtentMax = Math.max(snapshot.pipelineExtentMax, extent, 1);
}

function applyProgressPercentForEvent(
  snapshot: ProgressSnapshot,
  event: RuntimeJobEvent,
  payload: Record<string, unknown>,
  pct: number
): void {
  const innerFraction = ((snapshot.currentStage - 1) + pct / 100) / snapshot.totalStages;
  const intake = readIntakePipeline(payload);
  if (
    intake !== null &&
    intake.redactTotal > 0 &&
    event.commandName === 'scans.pdf-create-redacted'
  ) {
    const overall =
      50 * ((intake.redactIndex + innerFraction) / Math.max(1, intake.redactTotal));
    snapshot.progress = Math.round(overall);
    return;
  }
  if (intake !== null && intake.redactTotal === 0 && event.commandName === 'scans.ingest') {
    snapshot.progress = Math.round(50 + 50 * innerFraction);
    return;
  }
  const total = Math.max(1, snapshot.pipelineExtentMax);
  snapshot.progress = Math.round(((snapshot.pipelineJobsCompleted + innerFraction) / total) * 100);
}

function updateTrackedProgress(snapshot: ProgressSnapshot, event: RuntimeJobEvent): void {
  const payload = event.payload;
  extendPipelineExtentFromScheduler(snapshot, payload);

  const innerEvent = payload.event;
  if (innerEvent === 'started' && isRecord(event.payload.data)) {
    const totalStages = event.payload.data.total_stages;
    if (typeof totalStages === 'number') {
      snapshot.totalStages = totalStages;
    }
  }
  if (innerEvent === 'stage_started' && isRecord(event.payload.data)) {
    const stageNumber = event.payload.data.stage_number;
    if (typeof stageNumber === 'number') {
      snapshot.currentStage = stageNumber;
    }
  }
  if (event.payload.progress != null && isRecord(event.payload.progress)) {
    const pct = event.payload.progress.percent;
    if (typeof pct === 'number') {
      applyProgressPercentForEvent(snapshot, event, payload, pct);
    }
  }
}

function handleJobStartedOrSubmitted(snapshot: ProgressSnapshot, event: RuntimeJobEvent): void {
  const payload = event.payload;
  if ('schedulerActiveJobs' in payload || 'schedulerPendingJobs' in payload) {
    extendPipelineExtentFromScheduler(snapshot, payload);
  }
  const intake = readIntakePipeline(payload);
  if (
    intake?.active === true &&
    intake.redactTotal > 0 &&
    event.commandName === 'scans.pdf-create-redacted' &&
    intake.redactIndex > 0
  ) {
    snapshot.currentStage = 1;
    snapshot.totalStages = 1;
    return;
  }
  if (
    intake?.active === true &&
    intake.redactTotal === 0 &&
    event.commandName === 'scans.ingest'
  ) {
    snapshot.currentStage = 1;
    snapshot.totalStages = 1;
    snapshot.progress = Math.max(snapshot.progress ?? 0, 50);
    return;
  }
  applyTrackedProgress(snapshot);
}

function handleJobFinished(snapshot: ProgressSnapshot, event: RuntimeJobEvent): void {
  const payload = event.payload;
  const fromPythonWorker =
    'schedulerActiveJobs' in payload || 'schedulerPendingJobs' in payload;
  if (!fromPythonWorker) {
    if (event.commandName !== 'create_project') {
      resetProgress(snapshot);
    }
    return;
  }

  extendPipelineExtentFromScheduler(snapshot, payload);
  const intake = readIntakePipeline(payload);
  if (intake?.active === true) {
    if (event.commandName === 'scans.ingest' && intake.redactTotal === 0) {
      resetProgress(snapshot);
      return;
    }
    if (event.commandName === 'scans.pdf-create-redacted' && intake.redactTotal > 0) {
      snapshot.progress = Math.round(
        (50 * (intake.redactIndex + 1)) / Math.max(1, intake.redactTotal)
      );
      return;
    }
  }

  snapshot.pipelineJobsCompleted += 1;
  const total = Math.max(1, snapshot.pipelineExtentMax);
  snapshot.progress =
    snapshot.pipelineJobsCompleted >= total
      ? null
      : Math.round((snapshot.pipelineJobsCompleted / total) * 100);
  if (snapshot.pipelineJobsCompleted >= total) {
    resetProgress(snapshot);
  }
}

export interface RuntimeProgressTracker {
  getProgress(): number | null;
  handleEvent(event: RuntimeJobEvent): number | null;
  reset(): number | null;
}

export function createRuntimeProgressTracker(): RuntimeProgressTracker {
  const snapshot: ProgressSnapshot = {
    totalStages: 1,
    currentStage: 1,
    pipelineJobsCompleted: 0,
    pipelineExtentMax: 0,
    progress: null
  };

  return {
    getProgress() {
      return snapshot.progress;
    },
    handleEvent(event) {
      if (hiddenProgressCommands.has(event.commandName)) {
        return snapshot.progress;
      }
      if (event.eventType === 'job_queued') {
        const queuePosition = event.payload.queue_position ?? event.payload.queuePosition;
        if (typeof queuePosition === 'number') {
          snapshot.pipelineExtentMax = Math.max(snapshot.pipelineExtentMax, queuePosition);
        }
        return snapshot.progress;
      }
      if (event.eventType === 'job_started' || event.eventType === 'job_submitted') {
        handleJobStartedOrSubmitted(snapshot, event);
        return snapshot.progress;
      }
      if (event.eventType === 'job_progress') {
        updateTrackedProgress(snapshot, event);
        return snapshot.progress;
      }
      if (event.eventType === 'job_finished') {
        handleJobFinished(snapshot, event);
        return snapshot.progress;
      }
      if (
        event.eventType === 'job_failed' ||
        event.eventType === 'job_cancelled' ||
        event.eventType === 'job_skipped'
      ) {
        resetProgress(snapshot);
      }
      return snapshot.progress;
    },
    reset() {
      resetProgress(snapshot);
      return snapshot.progress;
    }
  };
}
