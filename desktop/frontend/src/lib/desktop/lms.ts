// SPDX-License-Identifier: AGPL-3.0-only
import type {
  LmsAssignmentSummary,
  LmsCourseSummary,
  LmsRosterCacheSnapshot,
  LmsRosterRow,
  ResultsLmsReportPreview,
  ResultsLmsUploadResponse,
  ResultsExportResponse,
  FinalizeReadyResultsInput,
  RunResultsExportInput,
  RetryResultsLmsUploadInput,
  RunResultsLmsUploadInput,
  SaveResultsLmsAssignmentInput,
  SetSubmissionResultFinalizedInput,
  ExamWorkspaceState,
  StudentRosterMatchResult
} from '$lib/types';

import {
  currentDesktopSettings,
  invokeDesktopHost,
  invokeDesktopHostWithSettings
} from './shared';

export function listCanvasCourses(
  baseUrl: string,
  accessToken: string
): Promise<LmsCourseSummary[]> {
  return invokeDesktopHost<LmsCourseSummary[]>('list_canvas_courses', {
    baseUrl,
    accessToken
  });
}

export function listCanvasCourseRoster(
  baseUrl: string,
  accessToken: string,
  courseId: string
): Promise<LmsRosterRow[]> {
  return invokeDesktopHost<LmsRosterRow[]>('list_canvas_course_roster', {
    baseUrl,
    accessToken,
    courseId
  });
}

export function computeLmsBindingToken(
  courseId: string,
  canvasUserId: string
): Promise<string> {
  return invokeDesktopHostWithSettings<string>(
    'compute_lms_binding_token',
    { courseId, canvasUserId },
    currentDesktopSettings()
  );
}

export function priorCanonicalSubmissionExistsForLmsStudent(
  courseId: string,
  canvasUserId: string
): Promise<boolean> {
  return invokeDesktopHostWithSettings<boolean>(
    'prior_canonical_submission_exists_for_lms_student',
    { courseId, canvasUserId },
    currentDesktopSettings()
  );
}

export function resolveLmsStudentRef(
  courseId: string,
  canvasUserId: string
): Promise<StudentRosterMatchResult> {
  return invokeDesktopHostWithSettings<StudentRosterMatchResult>(
    'resolve_lms_student_ref',
    { courseId, canvasUserId },
    currentDesktopSettings()
  );
}

export function getLmsRosterCacheState(): Promise<LmsRosterCacheSnapshot> {
  return invokeDesktopHostWithSettings<LmsRosterCacheSnapshot>(
    'get_lms_roster_cache_state',
    {},
    currentDesktopSettings()
  );
}

export function ensureLmsRosterPreload(): Promise<LmsRosterCacheSnapshot> {
  return invokeDesktopHostWithSettings<LmsRosterCacheSnapshot>(
    'ensure_lms_roster_preload',
    {},
    currentDesktopSettings()
  );
}

export function listLmsAssignments(): Promise<LmsAssignmentSummary[]> {
  return invokeDesktopHostWithSettings<LmsAssignmentSummary[]>(
    'list_lms_assignments',
    {},
    currentDesktopSettings()
  );
}

export function listLmsAssignmentsForCourse(courseId: string): Promise<LmsAssignmentSummary[]> {
  return invokeDesktopHostWithSettings<LmsAssignmentSummary[]>(
    'list_lms_assignments_for_course',
    { courseId },
    currentDesktopSettings()
  );
}

export function saveResultsLmsAssignment(
  input: SaveResultsLmsAssignmentInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHostWithSettings<ExamWorkspaceState>(
    'save_results_lms_assignment',
    { input },
    currentDesktopSettings()
  );
}

export function setSubmissionResultFinalized(
  input: SetSubmissionResultFinalizedInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('set_submission_result_finalized', { input });
}

export function finalizeReadyResults(
  input: FinalizeReadyResultsInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('finalize_ready_results', { input });
}

export function previewResultsLmsReport(
  studentRef: string
): Promise<ResultsLmsReportPreview> {
  return invokeDesktopHost<ResultsLmsReportPreview>('preview_results_lms_report', {
    studentRef,
  });
}

export function runResultsLmsUpload(
  input: RunResultsLmsUploadInput
): Promise<ResultsLmsUploadResponse> {
  return invokeDesktopHostWithSettings<ResultsLmsUploadResponse>(
    'run_results_lms_upload',
    { input },
    currentDesktopSettings()
  );
}

export function retryResultsLmsUpload(
  input: RetryResultsLmsUploadInput
): Promise<ResultsLmsUploadResponse> {
  return invokeDesktopHostWithSettings<ResultsLmsUploadResponse>(
    'retry_results_lms_upload',
    { input },
    currentDesktopSettings()
  );
}

export function runResultsExport(input: RunResultsExportInput): Promise<ResultsExportResponse> {
  return invokeDesktopHost<ResultsExportResponse>('run_results_export', { input });
}
