// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import type { ResultStudentRow } from '$lib/types';
import {
  filterResultsRows,
  sortResultsRows,
  uploadStatusSortRank,
} from './results-workspace-helpers';

function row(overrides: Partial<ResultStudentRow>): ResultStudentRow {
  return {
    studentRef: overrides.studentRef ?? 'student_1',
    aggregateTotal: overrides.aggregateTotal ?? 0,
    aggregateComplete: overrides.aggregateComplete ?? true,
    readyToFinalize: overrides.readyToFinalize ?? false,
    blockedReasons: overrides.blockedReasons ?? [],
    questionRows: overrides.questionRows ?? [],
    resultFingerprint: overrides.resultFingerprint ?? null,
    finalized: overrides.finalized ?? false,
    staleFinalization: overrides.staleFinalization ?? false,
    finalizedAt: overrides.finalizedAt ?? null,
    uploaded: overrides.uploaded ?? false,
    uploadFailed: overrides.uploadFailed ?? false,
    latestUploadError: overrides.latestUploadError ?? null,
    lastUploadAttemptId: overrides.lastUploadAttemptId ?? null,
  };
}

describe('results workspace helpers', () => {
  it('sorts rows by score and upload status', () => {
    const rows = [
      row({ studentRef: 'student_1', aggregateTotal: 77, readyToFinalize: true }),
      row({ studentRef: 'student_2', aggregateTotal: 91, uploaded: true, finalized: true }),
      row({ studentRef: 'student_3', aggregateTotal: 50, uploadFailed: true, finalized: true }),
    ];
    const displayNames = {
      student_1: 'Jordan',
      student_2: 'Alex',
      student_3: 'Morgan',
    };

    expect(
      sortResultsRows(rows, displayNames, 'score', 'desc').map((entry) => entry.studentRef)
    ).toEqual(['student_2', 'student_1', 'student_3']);

    expect(
      sortResultsRows(rows, displayNames, 'upload_status', 'asc').map((entry) => entry.studentRef)
    ).toEqual(['student_3', 'student_1', 'student_2']);
  });

  it('ranks upload states from attention to complete', () => {
    expect(uploadStatusSortRank(row({ uploadFailed: true }))).toBe(0);
    expect(uploadStatusSortRank(row({ staleFinalization: true }))).toBe(1);
    expect(uploadStatusSortRank(row({ readyToFinalize: true }))).toBe(2);
    expect(uploadStatusSortRank(row({ finalized: true }))).toBe(3);
    expect(uploadStatusSortRank(row({ finalized: true, uploaded: true }))).toBe(4);
  });

  it('filters rows by ready, finalized, and uploaded status buckets', () => {
    const rows = [
      row({ studentRef: 'student_1', readyToFinalize: true, finalized: false }),
      row({ studentRef: 'student_2', finalized: true, uploaded: false }),
      row({ studentRef: 'student_3', finalized: true, uploaded: true }),
      row({ studentRef: 'student_4', staleFinalization: true, finalized: true }),
    ];

    expect(filterResultsRows(rows, 'ready').map((entry) => entry.studentRef)).toEqual([
      'student_1',
    ]);
    expect(filterResultsRows(rows, 'finalized').map((entry) => entry.studentRef)).toEqual([
      'student_2',
    ]);
    expect(filterResultsRows(rows, 'uploaded').map((entry) => entry.studentRef)).toEqual([
      'student_3',
    ]);
    expect(filterResultsRows(rows, 'all').map((entry) => entry.studentRef)).toEqual([
      'student_1',
      'student_2',
      'student_3',
      'student_4',
    ]);
  });

  it('filters rows by display name and student ref search', () => {
    const rows = [
      row({ studentRef: 'student_1', finalized: true }),
      row({ studentRef: 'student_2', finalized: true }),
    ];
    const displayNames = {
      student_1: 'Jordan Scott',
      student_2: 'Alex Tobin',
    };

    expect(filterResultsRows(rows, 'all', 'jordan', displayNames).map((entry) => entry.studentRef)).toEqual([
      'student_1',
    ]);
    expect(filterResultsRows(rows, 'all', 'student_2', displayNames).map((entry) => entry.studentRef)).toEqual([
      'student_2',
    ]);
  });
});
