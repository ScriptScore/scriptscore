// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi } from 'vitest';

import { applyResultsUploadRuntimeEvent } from './resultsUploadRuntimeEvents';
import type { RuntimeJobEvent } from '$lib/types';

function runtimeEvent(
  eventType: string,
  payload: Record<string, unknown>,
  commandName = 'results.lms-upload'
): RuntimeJobEvent {
  return {
    eventType,
    commandName,
    workerStatus: 'busy',
    requestId: null,
    jobId: null,
    payload
  };
}

function progressStore() {
  return {
    startUploadBatch: vi.fn(),
    setStudentUploadProgress: vi.fn(),
    failUploadBatch: vi.fn(),
    finishUploadBatch: vi.fn()
  };
}

describe('applyResultsUploadRuntimeEvent', () => {
  it('starts the tracked upload batch', () => {
    const store = progressStore();

    const handled = applyResultsUploadRuntimeEvent(
      runtimeEvent('results_lms_upload_batch_started', {
        batchId: 'batch_1',
        studentRefs: ['student_1', 'student_2']
      }),
      store
    );

    expect(handled).toBe(true);
    expect(store.startUploadBatch).toHaveBeenCalledWith('batch_1', [
      'student_1',
      'student_2'
    ]);
  });

  it('tracks per-student progress updates', () => {
    const store = progressStore();

    const started = applyResultsUploadRuntimeEvent(
      runtimeEvent('results_lms_upload_student_started', {
        batchId: 'batch_1',
        studentRef: 'student_1'
      }),
      store
    );
    const finished = applyResultsUploadRuntimeEvent(
      runtimeEvent('results_lms_upload_student_finished', {
        batchId: 'batch_1',
        studentRef: 'student_1',
        status: 'uploaded'
      }),
      store
    );

    expect(started).toBe(true);
    expect(finished).toBe(true);
    expect(store.setStudentUploadProgress).toHaveBeenNthCalledWith(
      1,
      'batch_1',
      'student_1',
      'uploading'
    );
    expect(store.setStudentUploadProgress).toHaveBeenNthCalledWith(
      2,
      'batch_1',
      'student_1',
      'uploaded'
    );
  });

  it('marks the batch as failed or finished on terminal events', () => {
    const store = progressStore();

    applyResultsUploadRuntimeEvent(
      runtimeEvent('results_lms_upload_batch_failed', { batchId: 'batch_1' }),
      store
    );
    applyResultsUploadRuntimeEvent(
      runtimeEvent('results_lms_upload_batch_finished', { batchId: 'batch_1' }),
      store
    );

    expect(store.failUploadBatch).toHaveBeenCalledWith('batch_1');
    expect(store.finishUploadBatch).toHaveBeenCalledWith('batch_1');
  });

  it('ignores unrelated commands or malformed payloads', () => {
    const store = progressStore();

    expect(
      applyResultsUploadRuntimeEvent(
        runtimeEvent('results_lms_upload_batch_started', { batchId: 'batch_1' }, 'smoke.ping'),
        store
      )
    ).toBe(false);
    expect(
      applyResultsUploadRuntimeEvent(
        runtimeEvent('results_lms_upload_student_finished', {
          batchId: 'batch_1',
          studentRef: 'student_1',
          status: 'unknown'
        }),
        store
      )
    ).toBe(false);

    expect(store.startUploadBatch).not.toHaveBeenCalled();
    expect(store.setStudentUploadProgress).not.toHaveBeenCalled();
  });
});
