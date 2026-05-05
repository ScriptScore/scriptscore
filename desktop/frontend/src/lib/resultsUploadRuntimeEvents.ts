// SPDX-License-Identifier: AGPL-3.0-only
import { resultsWorkspaceView } from '$lib/stores/resultsWorkspaceView';
import type {
  ResultsUploadProgressStatus,
} from '$lib/stores/resultsWorkspaceView';
import type { RuntimeJobEvent } from '$lib/types';

export interface ResultsUploadProgressStore {
  startUploadBatch: (batchId: string, studentRefs: string[]) => void;
  setStudentUploadProgress: (
    batchId: string,
    studentRef: string,
    status: ResultsUploadProgressStatus
  ) => void;
  failUploadBatch: (batchId: string) => void;
  finishUploadBatch: (batchId: string) => void;
}

export function applyResultsUploadRuntimeEvent(
  event: RuntimeJobEvent,
  store: ResultsUploadProgressStore = resultsWorkspaceView
): boolean {
  if (event.commandName !== 'results.lms-upload') {
    return false;
  }

  const batchId = resultsUploadBatchIdFromEvent(event);
  if (!batchId) {
    return false;
  }

  if (event.eventType === 'results_lms_upload_batch_started') {
    store.startUploadBatch(batchId, resultsUploadStudentRefsFromEvent(event));
    return true;
  }

  if (event.eventType === 'results_lms_upload_student_started') {
    const studentRef = resultsUploadStudentRefFromEvent(event);
    if (!studentRef) {
      return false;
    }
    store.setStudentUploadProgress(batchId, studentRef, 'uploading');
    return true;
  }

  if (event.eventType === 'results_lms_upload_student_finished') {
    const studentRef = resultsUploadStudentRefFromEvent(event);
    const status = resultsUploadProgressStatusFromEvent(event);
    if (!studentRef || !status) {
      return false;
    }
    store.setStudentUploadProgress(batchId, studentRef, status);
    return true;
  }

  if (event.eventType === 'results_lms_upload_batch_failed') {
    store.failUploadBatch(batchId);
    return true;
  }

  if (event.eventType === 'results_lms_upload_batch_finished') {
    store.finishUploadBatch(batchId);
    return true;
  }

  return false;
}

function resultsUploadBatchIdFromEvent(event: RuntimeJobEvent): string | null {
  return typeof event.payload.batchId === 'string' && event.payload.batchId.length > 0
    ? event.payload.batchId
    : null;
}

function resultsUploadStudentRefsFromEvent(event: RuntimeJobEvent): string[] {
  return Array.isArray(event.payload.studentRefs)
    ? event.payload.studentRefs.filter((value): value is string => typeof value === 'string')
    : [];
}

function resultsUploadStudentRefFromEvent(event: RuntimeJobEvent): string | null {
  return typeof event.payload.studentRef === 'string' ? event.payload.studentRef : null;
}

function resultsUploadProgressStatusFromEvent(
  event: RuntimeJobEvent
): 'ready' | 'uploaded' | 'failed' | null {
  const status = typeof event.payload.status === 'string' ? event.payload.status : null;
  if (status === 'ready' || status === 'failed' || status === 'uploaded') {
    return status;
  }
  return null;
}
