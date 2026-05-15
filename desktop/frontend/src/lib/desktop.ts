// SPDX-License-Identifier: AGPL-3.0-only
export {
  RUNTIME_JOB_EVENT_NAME,
  isDesktopHost,
  toDesktopAssetUrl
} from './desktop/shared';
export { checkAppUpdate, getAppVersion, openExternalUrl } from './desktop/app';
export {
  createProject,
  getDefaultProjectsRoot,
  getExamWorkspaceState,
  getShellState,
  listVisionModels,
  validateVisionModel,
  openProject,
  projectExists,
  closeCurrentProject,
  runSmokePing,
  saveProjectConfig,
  saveQuestionEdits,
  saveRedactionRegions,
  approveTemplateSetup,
  ensureAutomaticRubricJobs,
  skipTemplateRedaction,
  generateQuestionRubric,
  reanalyzeQuestion,
  saveRubricUpdate,
  replaceTemplatePdf,
  exportStampedTemplatePdf
} from './desktop/project';
export {
  listCanvasCourses,
  listCanvasCourseRoster,
  computeLmsBindingToken,
  priorCanonicalSubmissionExistsForLmsStudent,
  resolveLmsStudentRef,
  getLmsRosterCacheState,
  ensureLmsRosterPreload,
  listLmsAssignments,
  listLmsAssignmentsForCourse,
  saveResultsLmsAssignment,
  setSubmissionResultFinalized,
  finalizeReadyResults,
  previewResultsLmsReport,
  runResultsLmsUpload,
  runResultsExport,
  retryResultsLmsUpload
} from './desktop/lms';
export {
  beginStudentWorkflow,
  confirmStudentAlignment,
  confirmStudentDetectReview,
  confirmStudentParseReview,
  saveStudentAlignmentReview,
  saveStudentDetectReview,
  saveStudentParseReview,
  deleteStudentSubmission,
  intakeDefaultPdfRectsFromTemplate,
  runStudentIntake,
  saveCriterionScore,
  saveModeratedFeedback,
  saveModeratedScore,
  saveStudentIntakePageOrder,
  setModerationQuestionReviewed,
  transientClipPdfRectsPngBase64,
  transientPdfClipText,
  transientRenderPdfPagePng,
  transientScansOcrHint
} from './desktop/students';
export {
  startJob,
  cancelActiveJob,
  getJobTrace,
  listJobTraces,
  listenRuntimeJobEvents
} from './desktop/runtime';
export { getLegalDisclosure } from './desktop/legal';
