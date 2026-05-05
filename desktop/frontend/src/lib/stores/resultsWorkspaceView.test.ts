// SPDX-License-Identifier: AGPL-3.0-only
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it } from 'vitest';

import { resultsPreviewEntry, resultsWorkspaceView } from './resultsWorkspaceView';

describe('resultsWorkspaceView', () => {
  beforeEach(() => {
    resultsWorkspaceView.reset();
  });

  it('resets view state when the project changes and preserves state for the same project', () => {
    resultsWorkspaceView.syncProject('/tmp/project-a');
    resultsWorkspaceView.setSearchTerm('Jordan');
    resultsWorkspaceView.setStatusFilter('ready');

    resultsWorkspaceView.syncProject('/tmp/project-a');
    expect(get(resultsWorkspaceView).searchTerm).toBe('Jordan');
    expect(get(resultsWorkspaceView).statusFilter).toBe('ready');

    resultsWorkspaceView.syncProject('/tmp/project-b');
    expect(get(resultsWorkspaceView)).toMatchObject({
      projectPath: '/tmp/project-b',
      searchTerm: '',
      statusFilter: 'all',
      selectedStudentRef: null,
      selectedStudentRefs: []
    });
  });

  it('syncs rows and attempts while preserving valid selections only', () => {
    resultsWorkspaceView.setSelectedStudentRef('student_1');
    resultsWorkspaceView.toggleStudentSelection('student_1');
    resultsWorkspaceView.toggleStudentSelection('stale_student');
    resultsWorkspaceView.setSelectedAttemptId('attempt_2');

    resultsWorkspaceView.syncRows(['student_1', 'student_2']);
    resultsWorkspaceView.syncAttempts(['attempt_1', 'attempt_2']);
    expect(get(resultsWorkspaceView)).toMatchObject({
      selectedStudentRef: 'student_1',
      selectedStudentRefs: ['student_1'],
      selectedAttemptId: 'attempt_2'
    });

    resultsWorkspaceView.syncRows(['student_2']);
    resultsWorkspaceView.syncAttempts(['attempt_3']);
    expect(get(resultsWorkspaceView)).toMatchObject({
      selectedStudentRef: null,
      selectedStudentRefs: [],
      selectedAttemptId: 'attempt_3'
    });

    resultsWorkspaceView.syncRows([]);
    resultsWorkspaceView.syncAttempts([]);
    expect(get(resultsWorkspaceView)).toMatchObject({
      selectedStudentRef: null,
      selectedStudentRefs: [],
      selectedAttemptId: null
    });
  });

  it('toggles checkbox selection and scoped select or unselect', () => {
    resultsWorkspaceView.toggleStudentSelection('student_1');
    resultsWorkspaceView.toggleStudentSelection('student_2');
    resultsWorkspaceView.toggleStudentSelection('student_1');
    expect(get(resultsWorkspaceView).selectedStudentRefs).toEqual(['student_2']);

    resultsWorkspaceView.setStudentSelectionForScope(['student_1', 'student_3'], true);
    expect(get(resultsWorkspaceView).selectedStudentRefs).toEqual([
      'student_2',
      'student_1',
      'student_3'
    ]);

    resultsWorkspaceView.setStudentSelectionForScope(['student_2', 'student_3'], false);
    expect(get(resultsWorkspaceView).selectedStudentRefs).toEqual(['student_1']);
  });

  it('updates sort, filter, search, display, export, and warning toggles', () => {
    resultsWorkspaceView.setSortKey('score');
    resultsWorkspaceView.setSortDirection('desc');
    resultsWorkspaceView.setStatusFilter('uploaded');
    resultsWorkspaceView.setSearchTerm('alex');
    resultsWorkspaceView.setScoreDisplayMode('points');
    resultsWorkspaceView.setShowExportActionForLms(true);
    resultsWorkspaceView.flashUploadWarning();
    resultsWorkspaceView.flashUploadWarning();

    expect(get(resultsWorkspaceView)).toMatchObject({
      sortKey: 'score',
      sortDirection: 'desc',
      statusFilter: 'uploaded',
      searchTerm: 'alex',
      scoreDisplayMode: 'points',
      showExportActionForLms: true,
      uploadWarningFlashCount: 2
    });
  });

  it('tracks preview loading, success, error, lookup, and reset', () => {
    expect(resultsPreviewEntry(null)).toBeNull();
    expect(resultsPreviewEntry('student_1')).toBeNull();

    resultsWorkspaceView.startPreview('student_1', 'fp_1');
    expect(resultsPreviewEntry('student_1')).toEqual({
      status: 'loading',
      resultFingerprint: 'fp_1',
      html: null,
      error: null
    });

    resultsWorkspaceView.savePreview('student_1', 'fp_1', '<html>ready</html>');
    expect(resultsPreviewEntry('student_1')).toMatchObject({
      status: 'ready',
      html: '<html>ready</html>',
      error: null
    });

    resultsWorkspaceView.setPreviewError('student_2', 'fp_2', 'Preview failed');
    expect(resultsPreviewEntry('student_2')).toEqual({
      status: 'error',
      resultFingerprint: 'fp_2',
      html: null,
      error: 'Preview failed'
    });

    resultsWorkspaceView.resetPreviewCache();
    expect(get(resultsWorkspaceView).previewByStudentRef).toEqual({});
  });

  it('tracks upload batches, ignores stale batches, and fails active uploads', () => {
    resultsWorkspaceView.startUploadBatch('batch_1', ['student_1', 'student_2']);
    expect(get(resultsWorkspaceView)).toMatchObject({
      activeUploadBatchId: 'batch_1',
      uploadProgressByStudentRef: {
        student_1: 'uploading',
        student_2: 'uploading'
      }
    });

    resultsWorkspaceView.setStudentUploadProgress('stale_batch', 'student_1', 'uploaded');
    expect(get(resultsWorkspaceView).uploadProgressByStudentRef.student_1).toBe('uploading');

    resultsWorkspaceView.setStudentUploadProgress('batch_1', 'student_1', 'uploaded');
    resultsWorkspaceView.finishUploadBatch('stale_batch');
    expect(get(resultsWorkspaceView)).toMatchObject({
      activeUploadBatchId: 'batch_1',
      uploadProgressByStudentRef: {
        student_1: 'uploaded',
        student_2: 'uploading'
      }
    });

    resultsWorkspaceView.finishUploadBatch('batch_1');
    expect(get(resultsWorkspaceView).activeUploadBatchId).toBeNull();

    resultsWorkspaceView.startUploadBatch('batch_2', ['student_1', 'student_2']);
    resultsWorkspaceView.setStudentUploadProgress('batch_2', 'student_1', 'uploaded');
    resultsWorkspaceView.failUploadBatch('stale_batch');
    expect(get(resultsWorkspaceView).activeUploadBatchId).toBe('batch_2');

    resultsWorkspaceView.failUploadBatch('batch_2');
    expect(get(resultsWorkspaceView)).toMatchObject({
      activeUploadBatchId: null,
      uploadProgressByStudentRef: {
        student_1: 'uploaded',
        student_2: 'failed'
      }
    });

    resultsWorkspaceView.startUploadBatch('batch_3', ['student_3']);
    resultsWorkspaceView.failActiveUploadBatch();
    expect(get(resultsWorkspaceView)).toMatchObject({
      activeUploadBatchId: null,
      uploadProgressByStudentRef: {
        student_3: 'failed'
      }
    });

    resultsWorkspaceView.clearUploadProgress();
    expect(get(resultsWorkspaceView)).toMatchObject({
      activeUploadBatchId: null,
      uploadProgressByStudentRef: {}
    });
  });
});
