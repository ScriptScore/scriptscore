// SPDX-License-Identifier: AGPL-3.0-only
import type { ResultStudentRow } from '$lib/types';
import type {
  ResultsSortDirection,
  ResultsSortKey,
  ResultsStatusFilter,
} from '$lib/stores/resultsWorkspaceView';

export function displayNameForStudent(
  studentRef: string,
  studentDisplayNamesByRef: Record<string, string>
): string {
  return studentDisplayNamesByRef[studentRef]?.trim() || studentRef;
}

export function uploadStatusSortRank(row: ResultStudentRow): number {
  if (row.uploadFailed) {
    return 0;
  }
  if (row.staleFinalization) {
    return 1;
  }
  if (row.readyToFinalize && !row.finalized) {
    return 2;
  }
  if (row.finalized && !row.uploaded) {
    return 3;
  }
  if (row.uploaded) {
    return 4;
  }
  return 5;
}

export function sortResultsRows(
  rows: ResultStudentRow[],
  studentDisplayNamesByRef: Record<string, string>,
  sortKey: ResultsSortKey,
  sortDirection: ResultsSortDirection
): ResultStudentRow[] {
  const direction = sortDirection === 'asc' ? 1 : -1;
  return [...rows].sort((left, right) => {
    const comparison = compareRows(left, right, studentDisplayNamesByRef, sortKey);
    if (comparison !== 0) {
      return comparison * direction;
    }
    return (
      displayNameForStudent(left.studentRef, studentDisplayNamesByRef).localeCompare(
        displayNameForStudent(right.studentRef, studentDisplayNamesByRef)
      ) || left.studentRef.localeCompare(right.studentRef)
    );
  });
}

export function filterResultsRows(
  rows: ResultStudentRow[],
  statusFilter: ResultsStatusFilter,
  searchTerm = '',
  studentDisplayNamesByRef: Record<string, string> = {}
): ResultStudentRow[] {
  const normalizedSearch = searchTerm.trim().toLocaleLowerCase();
  return rows.filter(
    (row) =>
      rowMatchesStatusFilter(row, statusFilter) &&
      rowMatchesSearch(row, normalizedSearch, studentDisplayNamesByRef)
  );
}

export function rowMatchesStatusFilter(
  row: ResultStudentRow,
  statusFilter: ResultsStatusFilter
): boolean {
  switch (statusFilter) {
    case 'ready':
      return row.readyToFinalize && !row.finalized;
    case 'finalized':
      return row.finalized && !row.staleFinalization && !row.uploaded;
    case 'uploaded':
      return row.uploaded;
    case 'all':
    default:
      return true;
  }
}

export function statusFilterLabel(statusFilter: ResultsStatusFilter): string {
  switch (statusFilter) {
    case 'ready':
      return 'Ready';
    case 'finalized':
      return 'Finalized';
    case 'uploaded':
      return 'Uploaded';
    case 'all':
    default:
      return 'All';
  }
}

function rowMatchesSearch(
  row: ResultStudentRow,
  normalizedSearch: string,
  studentDisplayNamesByRef: Record<string, string>
): boolean {
  if (normalizedSearch.length === 0) {
    return true;
  }
  return (
    displayNameForStudent(row.studentRef, studentDisplayNamesByRef)
      .toLocaleLowerCase()
      .includes(normalizedSearch) || row.studentRef.toLocaleLowerCase().includes(normalizedSearch)
  );
}

function compareRows(
  left: ResultStudentRow,
  right: ResultStudentRow,
  studentDisplayNamesByRef: Record<string, string>,
  sortKey: ResultsSortKey
): number {
  switch (sortKey) {
    case 'score':
      return compareScores(left, right);
    case 'upload_status':
      return uploadStatusSortRank(left) - uploadStatusSortRank(right);
    case 'name':
    default:
      return displayNameForStudent(left.studentRef, studentDisplayNamesByRef).localeCompare(
        displayNameForStudent(right.studentRef, studentDisplayNamesByRef)
      );
  }
}

function compareScores(left: ResultStudentRow, right: ResultStudentRow): number {
  if (left.aggregateComplete && right.aggregateComplete) {
    return left.aggregateTotal - right.aggregateTotal;
  }
  if (left.aggregateComplete) {
    return -1;
  }
  if (right.aggregateComplete) {
    return 1;
  }
  return 0;
}

export function formatMetricScore(value: number | null | undefined, digits = 2): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return 'N/A';
  }
  return value.toFixed(digits);
}

export function formatPercent(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return 'N/A';
  }
  return `${value.toFixed(1)}%`;
}

export function aggregateMaxPoints(row: ResultStudentRow): number | null {
  const maxima = row.questionRows
    .map((questionRow) => questionRow.maxPoints)
    .filter((value): value is number => value !== null && value !== undefined);
  if (maxima.length !== row.questionRows.length || maxima.length === 0) {
    return null;
  }
  return maxima.reduce((total, value) => total + value, 0);
}

export function aggregateScoreLabel(row: ResultStudentRow): string {
  if (!row.aggregateComplete) {
    return 'Pending';
  }
  const maxPoints = aggregateMaxPoints(row);
  return maxPoints === null ? `${row.aggregateTotal} pts` : `${row.aggregateTotal} / ${maxPoints}`;
}

export function clampPercent(value: number | null | undefined): number {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return 0;
  }
  return Math.min(100, Math.max(0, value));
}

export function aggregateScorePercent(row: ResultStudentRow): number {
  if (!row.aggregateComplete) {
    return 0;
  }
  const maxPoints = aggregateMaxPoints(row);
  if (maxPoints === null || maxPoints <= 0) {
    return 0;
  }
  return clampPercent((row.aggregateTotal / maxPoints) * 100);
}

export function formatPercentValue(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return 'N/A';
  }
  return `${value.toFixed(digits)}%`;
}

export function formatScoreDisplay(
  value: number | null | undefined,
  percentValue: number | null | undefined,
  mode: 'percent' | 'points',
  digits = 1
): string {
  if (mode === 'percent') {
    return formatPercentValue(percentValue, digits);
  }
  return formatMetricScore(value, digits);
}

export function examMaxPointsFromQuestions(
  questions: Array<{ maxPoints: number | null | undefined }>
): number | null {
  const maxima = questions
    .map((question) => question.maxPoints)
    .filter((value): value is number => value !== null && value !== undefined);
  if (maxima.length !== questions.length || maxima.length === 0) {
    return null;
  }
  return maxima.reduce((total, value) => total + value, 0);
}
