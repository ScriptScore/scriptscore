// SPDX-License-Identifier: AGPL-3.0-only
import { get, writable } from 'svelte/store';

export type ResultsSortKey = 'name' | 'score' | 'upload_status';
export type ResultsSortDirection = 'asc' | 'desc';
export type ResultsStatusFilter = 'all' | 'ready' | 'finalized' | 'uploaded';
export type ResultsPreviewStatus = 'idle' | 'loading' | 'ready' | 'error';
export type ResultsScoreDisplayMode = 'percent' | 'points';
export type ResultsUploadProgressStatus = 'ready' | 'uploading' | 'uploaded' | 'failed';

export interface ResultsPreviewEntry {
  status: ResultsPreviewStatus;
  resultFingerprint: string | null;
  html: string | null;
  error: string | null;
}

export interface ResultsWorkspaceViewState {
  projectPath: string | null;
  selectedStudentRef: string | null;
  selectedStudentRefs: string[];
  sortKey: ResultsSortKey;
  sortDirection: ResultsSortDirection;
  statusFilter: ResultsStatusFilter;
  searchTerm: string;
  selectedAttemptId: string | null;
  scoreDisplayMode: ResultsScoreDisplayMode;
  showExportActionForLms: boolean;
  previewByStudentRef: Record<string, ResultsPreviewEntry>;
  activeUploadBatchId: string | null;
  uploadProgressByStudentRef: Record<string, ResultsUploadProgressStatus>;
  uploadWarningFlashCount: number;
}

const initialState: ResultsWorkspaceViewState = {
  projectPath: null,
  selectedStudentRef: null,
  selectedStudentRefs: [],
  sortKey: 'name',
  sortDirection: 'asc',
  statusFilter: 'all',
  searchTerm: '',
  selectedAttemptId: null,
  scoreDisplayMode: 'percent',
  showExportActionForLms: false,
  previewByStudentRef: {},
  activeUploadBatchId: null,
  uploadProgressByStudentRef: {},
  uploadWarningFlashCount: 0,
};

function createResultsWorkspaceViewStore() {
  const { subscribe, set, update } = writable<ResultsWorkspaceViewState>(initialState);

  return {
    subscribe,
    reset() {
      set(initialState);
    },
    syncProject(projectPath: string | null) {
      update((current) => {
        if (current.projectPath === projectPath) {
          return current;
        }
        return {
          ...initialState,
          projectPath,
        };
      });
    },
    syncRows(studentRefs: string[]) {
      update((current) => {
        if (studentRefs.length === 0) {
          return {
            ...current,
            selectedStudentRef: null,
            selectedStudentRefs: [],
          };
        }
        const selectedStudentRef =
          current.selectedStudentRef !== null &&
          studentRefs.includes(current.selectedStudentRef)
            ? current.selectedStudentRef
            : null;
        const selectedStudentRefs = current.selectedStudentRefs.filter((studentRef) =>
          studentRefs.includes(studentRef)
        );
        return {
          ...current,
          selectedStudentRef,
          selectedStudentRefs,
        };
      });
    },
    syncAttempts(attemptIds: string[]) {
      update((current) => ({
        ...current,
        selectedAttemptId: attemptIds.includes(current.selectedAttemptId ?? '')
          ? current.selectedAttemptId
          : attemptIds[0] ?? null,
      }));
    },
    setSelectedStudentRef(studentRef: string | null) {
      update((current) => ({ ...current, selectedStudentRef: studentRef }));
    },
    toggleStudentSelection(studentRef: string) {
      update((current) => ({
        ...current,
        selectedStudentRefs: current.selectedStudentRefs.includes(studentRef)
          ? current.selectedStudentRefs.filter((entry) => entry !== studentRef)
          : [...current.selectedStudentRefs, studentRef],
      }));
    },
    setStudentSelectionForScope(studentRefs: string[], selected: boolean) {
      update((current) => {
        const scope = new Set(studentRefs);
        const selectedSet = new Set(current.selectedStudentRefs);
        if (selected) {
          for (const studentRef of scope) {
            selectedSet.add(studentRef);
          }
        } else {
          for (const studentRef of scope) {
            selectedSet.delete(studentRef);
          }
        }
        return {
          ...current,
          selectedStudentRefs: [...selectedSet],
        };
      });
    },
    setSortKey(sortKey: ResultsSortKey) {
      update((current) => ({ ...current, sortKey }));
    },
    setSortDirection(sortDirection: ResultsSortDirection) {
      update((current) => ({ ...current, sortDirection }));
    },
    setStatusFilter(statusFilter: ResultsStatusFilter) {
      update((current) => ({ ...current, statusFilter }));
    },
    setSearchTerm(searchTerm: string) {
      update((current) => ({ ...current, searchTerm }));
    },
    setSelectedAttemptId(attemptId: string | null) {
      update((current) => ({ ...current, selectedAttemptId: attemptId }));
    },
    setScoreDisplayMode(scoreDisplayMode: ResultsScoreDisplayMode) {
      update((current) => ({ ...current, scoreDisplayMode }));
    },
    setShowExportActionForLms(showExportActionForLms: boolean) {
      update((current) => ({ ...current, showExportActionForLms }));
    },
    resetPreviewCache() {
      update((current) => ({ ...current, previewByStudentRef: {} }));
    },
    clearUploadProgress() {
      update((current) => ({
        ...current,
        activeUploadBatchId: null,
        uploadProgressByStudentRef: {},
      }));
    },
    startUploadBatch(batchId: string, studentRefs: string[]) {
      update((current) => ({
        ...current,
        activeUploadBatchId: batchId,
        uploadProgressByStudentRef: Object.fromEntries(
          studentRefs.map((studentRef) => [studentRef, 'uploading' as const])
        ),
      }));
    },
    setStudentUploadProgress(
      batchId: string,
      studentRef: string,
      status: ResultsUploadProgressStatus
    ) {
      update((current) => {
        if (current.activeUploadBatchId !== batchId) {
          return current;
        }
        return {
          ...current,
          uploadProgressByStudentRef: {
            ...current.uploadProgressByStudentRef,
            [studentRef]: status,
          },
        };
      });
    },
    finishUploadBatch(batchId: string) {
      update((current) =>
        current.activeUploadBatchId === batchId
          ? { ...current, activeUploadBatchId: null }
          : current
      );
    },
    failUploadBatch(batchId: string) {
      update((current) => {
        if (current.activeUploadBatchId !== batchId) {
          return current;
        }
        const uploadProgressByStudentRef = { ...current.uploadProgressByStudentRef };
        for (const studentRef of Object.keys(uploadProgressByStudentRef)) {
          if (uploadProgressByStudentRef[studentRef] === 'uploading') {
            uploadProgressByStudentRef[studentRef] = 'failed';
          }
        }
        return {
          ...current,
          activeUploadBatchId: null,
          uploadProgressByStudentRef,
        };
      });
    },
    failActiveUploadBatch() {
      update((current) => {
        if (current.activeUploadBatchId === null) {
          return current;
        }
        const uploadProgressByStudentRef = { ...current.uploadProgressByStudentRef };
        for (const studentRef of Object.keys(uploadProgressByStudentRef)) {
          if (uploadProgressByStudentRef[studentRef] === 'uploading') {
            uploadProgressByStudentRef[studentRef] = 'failed';
          }
        }
        return {
          ...current,
          activeUploadBatchId: null,
          uploadProgressByStudentRef,
        };
      });
    },
    startPreview(studentRef: string, resultFingerprint: string | null) {
      update((current) => ({
        ...current,
        previewByStudentRef: {
          ...current.previewByStudentRef,
          [studentRef]: {
            status: 'loading',
            resultFingerprint,
            html: null,
            error: null,
          },
        },
      }));
    },
    savePreview(studentRef: string, resultFingerprint: string | null, html: string) {
      update((current) => ({
        ...current,
        previewByStudentRef: {
          ...current.previewByStudentRef,
          [studentRef]: {
            status: 'ready',
            resultFingerprint,
            html,
            error: null,
          },
        },
      }));
    },
    setPreviewError(studentRef: string, resultFingerprint: string | null, error: string) {
      update((current) => ({
        ...current,
        previewByStudentRef: {
          ...current.previewByStudentRef,
          [studentRef]: {
            status: 'error',
            resultFingerprint,
            html: null,
            error,
          },
        },
      }));
    },
    flashUploadWarning() {
      update((current) => ({
        ...current,
        uploadWarningFlashCount: current.uploadWarningFlashCount + 1,
      }));
    },
  };
}

export const resultsWorkspaceView = createResultsWorkspaceViewStore();

export function resultsPreviewEntry(studentRef: string | null): ResultsPreviewEntry | null {
  if (!studentRef) {
    return null;
  }
  return get(resultsWorkspaceView).previewByStudentRef[studentRef] ?? null;
}
