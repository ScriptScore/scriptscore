// SPDX-License-Identifier: AGPL-3.0-only
export type WorkerStatus = 'starting' | 'ready' | 'busy' | 'error';

export interface ProjectSummary {
  projectId: string;
  displayName: string;
  subject: string | null;
  courseCode: string | null;
  /** Canvas (or other LMS) course id when linked from LMS picker. */
  lmsCourseId: string | null;
  projectPath: string;
  createdAt: string;
  updatedAt: string;
}

export interface ShellState {
  currentProject: ProjectSummary | null;
  workerStatus: WorkerStatus;
  workerActivity?: WorkerActivity;
  lastRuntimeError: string | null;
  debugFeatures: DebugFeatures;
}

export interface WorkerActivity {
  activeJobs: WorkerJobSummary[];
  pendingJobCount: number;
}

export interface WorkerJobSummary {
  jobId: string;
  commandName: string;
  startedAt?: string | null;
}

export interface DebugFeatures {
  redactionToggle: boolean;
}

export interface LegalDisclosure {
  licenseExpression: string;
  sourceUrl: string;
  localNoticesPath: string;
  thirdPartyNotices: string;
  policyReportJson: string;
  artifactStatus: string;
}

export type AppUpdateStatus =
  | 'up_to_date'
  | 'update_available'
  | 'no_stable_release'
  | 'unavailable';

export interface AppUpdateCheck {
  installedVersion: string;
  latestStableVersion: string | null;
  latestStableTag: string | null;
  releaseUrl: string | null;
  updateAvailable: boolean;
  status: AppUpdateStatus;
  message: string;
}

export interface InstructorProfile {
  gradingStrictness: string;
  syntaxLeniency: string;
  ocrTolerance: string;
  partialCreditStyle: string;
  feedbackStyle: string;
  enabledTags: InstructorProfileEnabledTags;
  additionalGuidance: string;
  includeMinimumCreditCriterion: boolean;
  /** Percent of question max points; host rounds to integer points when first applying minimum-credit row. */
  minimumCreditPercent: number;
}

export interface InstructorProfileEnabledTags {
  gradingStrictness: boolean;
  syntaxLeniency: boolean;
  ocrTolerance: boolean;
  partialCreditStyle: boolean;
  feedbackStyle: boolean;
}

export type LmsProviderId = 'none' | 'canvas';

export interface AppSettings {
  llmProvider: string;
  llmBaseUrl: string;
  llmModel: string;
  llmApiKey: string | null;
  /** LMS integration for course lists (Canvas first). */
  lmsProvider: LmsProviderId;
  lmsCanvasBaseUrl: string;
  lmsCanvasApiKey: string | null;
  /** When enabled, mirror the LMS binding HMAC secret to a plaintext file as a recovery fallback. */
  lmsBindingSecretPlaintextFallback: boolean;
  /** Optional desktop override for the local Paddle OCR model directory used by scans.pii. */
  piiPaddleModelDir: string | null;
  /** Opt-in worker count for answer-scoped preliminary grading rows. */
  preliminaryGradingMaxWorkers: number;
  /** Optional desktop root directory for newly created ScriptScore projects. */
  projectsDirectory: string | null;
  instructorProfile: InstructorProfile;
  aiAssistEnabled: boolean;
  onboardingCompleted: boolean;
  aiAssistCategories: AiAssistCategories;
  theme: string;
}

export interface AiAssistCategories {
  rubrics: boolean;
  questionAnalysis: boolean;
  gradingFeedback: boolean;
  parsingReview: boolean;
}

export interface CreateProjectInput {
  displayName: string;
  subject: string | null;
  courseCode: string | null;
  lmsCourseId: string | null;
  projectRoot: string | null;
  templatePdfPath: string;
  instructorProfile?: InstructorProfile;
}

export interface SmokePingResult {
  command: string;
  message: string;
  steps: number;
  eventCount: number;
}

/** Transient `scans.ocr` via Tauri (not stored in the project job table). */
export interface ScansOcrHintResult {
  hintText: string;
  /** Rows returned by EasyOCR `readtext` (0 when test env bypasses OCR). */
  segmentCount: number;
}

export interface VisionCapableModel {
  name: string;
  displayName: string;
}

export interface LlmModelValidation {
  model: string;
  displayName: string;
  capabilities: string[];
  valid: boolean;
  reason: string | null;
  missingCapabilities: string[];
}

export interface WorkspaceWarning {
  code: string | null;
  message: string;
  scope: string | null;
}

export interface RuntimeJobEvent {
  eventType: string;
  commandName: string;
  workerStatus: WorkerStatus;
  requestId: string | null;
  jobId: string | null;
  payload: Record<string, unknown>;
}

export interface TemplatePageArtifactSummary {
  artifactId: string;
  pageNumber: number;
  imagePath: string;
  label: string;
}

export interface TemplateArucoPageStatus {
  pageNumber: number;
  markerCount: number;
  markerIds: number[];
}

export interface TemplateArucoStatus {
  state: 'unknown' | 'detected' | 'not_detected' | (string & {});
  totalMarkerCount: number;
  pages: TemplateArucoPageStatus[];
}

export interface QuestionRecord {
  questionId: string;
  questionNumber: number;
  pageNumber: number;
  maxPoints: number | null;
  text: string;
  baselinePdfText: string;
  region?: TemplateQuestionRegion | null;
  sourceArtifactId: string | null;
  imagePath?: string | null;
  analysis?: QuestionAnalysisState;
  rubric?: RubricState;
}

export interface TemplateQuestionRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface QuestionEdit {
  questionId: string;
  questionNumber: number;
  pageNumber: number;
  maxPoints: number | null;
  text: string;
  questionContext: string;
  rubricEditImpact?: 'minor' | 'grading' | null;
}

/** The first redaction region (lowest sort_order) is dedicated to identifying student names. */
export const REDACTION_LABEL_NAME_IDENTIFICATION = 'name_identification';
/** Additional redaction regions beyond the first are pre-emptive privacy masks. */
export const REDACTION_LABEL_PRIVACY_PROTECTION = 'privacy_protection';

export interface TemplateRedactionRegion {
  regionId: string;
  pageNumber: number;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Semantic label: `name_identification` for the first region, `privacy_protection` for others. */
  label: string;
  sortOrder: number;
}

export interface TemplateRedactionRegionInput {
  regionId: string | null;
  pageNumber: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ProjectTraceRefs {
  setupJobId: string | null;
  batchAnalyzeJobId: string | null;
  batchRubricJobId: string | null;
  intakeJobId: string | null;
}

export interface ProjectConfig {
  projectId: string;
  displayName: string;
  subject: string | null;
  courseCode: string | null;
  lmsCourseId: string | null;
  lmsAssignmentId?: string | null;
  redactionRequired: boolean;
  instructorProfile: InstructorProfile;
  traceRefs: ProjectTraceRefs;
  createdAt: string;
  updatedAt: string;
}

/** Course row from the desktop host (Canvas API normalized). */
export interface LmsCourseSummary {
  lmsCourseId: string;
  name: string;
  courseCode: string | null;
}

export interface LmsAssignmentSummary {
  assignmentId: string;
  name: string;
  pointsPossible?: number | null;
}

/** One student row from Canvas (transient UI; do not persist names in project DB). */
export interface LmsRosterRow {
  userId: string;
  displayName: string;
  sortKey: string;
  email?: string | null;
  loginId?: string | null;
}

export interface PdfPointRect {
  pageNumber: number;
  xPt: number;
  yPt: number;
  widthPt: number;
  heightPt: number;
}

export interface IntakePreviewPage {
  pageNumber: number;
  /** Total pages in the source PDF (reported on every render response). */
  pageCount: number;
  pageWidthPt: number;
  pageHeightPt: number;
  pngWidthPx: number;
  pngHeightPx: number;
  pngBase64: string;
}

export interface StudentIntakeRedactionRegionInput {
  pageNumber: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface StudentIntakeRasterSize {
  widthPx: number;
  heightPx: number;
}

export interface ExamWorkspaceState {
  project: ProjectSummary;
  status: string;
  statusLabel: string;
  failureMessage: string | null;
  templatePreviewArtifacts: TemplatePageArtifactSummary[];
  arucoStatus?: TemplateArucoStatus;
  questions: QuestionRecord[];
  redactionRegions: TemplateRedactionRegion[];
  warnings: WorkspaceWarning[];
  canApprove: boolean;
  /** When true, rubric save/approve actions are allowed (draft or approved template). */
  canApproveRubric: boolean;
  projectConfig?: ProjectConfig;
  studentRoster?: StudentRosterRow[];
  studentIntake?: StudentIntakeState;
  studentWorkflow?: StudentWorkflowState;
  moderationState?: ModerationState;
  resultsLmsState?: ResultsLmsState;
  resultsLmsRows?: ResultStudentRow[];
  resultsLmsMetrics?: ResultsExamMetrics | null;
  resultsLmsReviewSummary?: ResultsLmsReviewSummary | null;
  workflowStage?: string;
  workflowLabel?: string;
}

export interface QuestionAnalysisState {
  status: string;
  questionTextClean: string | null;
  questionContext: string | null;
  warnings: WorkspaceWarning[];
  latestJobId: string | null;
}

export interface RubricCriterion {
  criterionId: string;
  label: string;
  points: number;
  partialCreditGuidance: string;
  source: string;
}

export interface RubricApprovalBasis {
  questionText: string;
  questionContext: string;
  maxPoints: number | null;
  criteria: RubricCriterion[];
}

export interface RubricState {
  status: string;
  criteria: RubricCriterion[];
  warnings: WorkspaceWarning[];
  approvedAt: string | null;
  latestJobId: string | null;
  approvalBasis?: RubricApprovalBasis | null;
}

export interface StudentIntakeSummary {
  studentRef: string;
  localDisplayName?: string | null;
  canonicalPdfPath: string;
  ingestStatus: string;
  pageCount: number;
  /** Absolute paths to scans.ingest page PNGs in operator-confirmed intake order. */
  examPagePaths?: string[];
  warnings: WorkspaceWarning[];
  /** Hex HMAC binding token when persisted; never raw LMS identifiers. */
  bindingTokenHex?: string | null;
}

export interface StudentIntakeState {
  status: string;
  latestJobId: string | null;
  items: StudentIntakeSummary[];
  unresolvedCount: number;
}

export interface StudentRosterRow {
  studentRef: string;
  bindingTokenHex: string;
}

export interface StudentRosterMatchResult {
  studentRef: string;
  bindingTokenHex: string;
}

export type LmsRosterCacheStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface LmsRosterCacheSnapshot {
  status: LmsRosterCacheStatus;
  projectPath: string | null;
  lmsProvider: string | null;
  courseId: string | null;
  rows: LmsRosterRow[];
  lastError: string | null;
  idleReason: string | null;
}

export interface StudentWorkflowTransform {
  rotation: number;
  scale: number;
  translateX: number;
  translateY: number;
}

export interface StudentWorkflowPage {
  pageNumber: number;
  imagePath: string;
  sourcePdfPath: string | null;
  ocrMetadataPath?: string | null;
}

export interface StudentWorkflowAlignmentPage {
  pageNumber: number;
  confidence: number | null;
  lowConfidence: boolean;
  reviewExempt?: boolean;
  reviewExemptReason?: string | null;
  questionCount?: number;
  transform: StudentWorkflowTransform;
  warnings: WorkspaceWarning[];
}

export interface StudentWorkflowCriterionResult {
  criterionIndex: number;
  label?: string;
  points: number;
  pointsAwarded: number;
  rationale: string;
}

export interface StudentWorkflowHighlightSpan {
  kind: string;
  startChar: number;
  endChar: number;
  text: string;
}

export interface StudentWorkflowPiiPrescreen {
  sourceCommand: string;
  status: string;
  containsHandwriting: string;
  containsPii: boolean;
  piiTypesDetected: string[];
  warnings: WorkspaceWarning[];
}

export interface StudentWorkflowAnswer {
  questionId: string;
  questionNumber: number;
  cropImagePath: string | null;
  piiPrescreen?: StudentWorkflowPiiPrescreen | null;
  manualGradingRequired?: boolean;
  manualGradingReason?: string | null;
  moderationEligible?: boolean;
  parseStatus: string;
  parseConfidence: string | null;
  parseConfidenceSource: string | null;
  rawParsedText: string | null;
  verifiedText: string | null;
  reviewRequired: boolean;
  verified: boolean;
  stale: boolean;
  gradingStatus: string;
  gradingConfidence: string | null;
  gradingConfidenceReason: string | null;
  questionMaxPoints: number | null;
  totalPointsAwarded: number | null;
  feedbackText: string | null;
  criterionResults: StudentWorkflowCriterionResult[];
  highlights: StudentWorkflowHighlightSpan[];
  warnings: WorkspaceWarning[];
}

export interface ModerationScoreOverride {
  studentRef: string;
  questionId: string;
  moderatedTotalPoints: number;
}

export interface ModerationFeedbackOverride {
  studentRef: string;
  questionId: string;
  feedbackText: string;
}

export interface ModerationQuestionReview {
  questionId: string;
  reviewedAt: string;
}

export interface ModerationState {
  scoreOverrides: ModerationScoreOverride[];
  feedbackOverrides: ModerationFeedbackOverride[];
  questionReviews: ModerationQuestionReview[];
}

export type LmsUploadMode = 'dry_run' | 'live';
export type LmsUploadStudentStatus = 'ready' | 'uploaded' | 'failed';

export interface ResultsLmsTarget {
  provider: string;
  courseId: string;
  assignmentId: string;
}

export interface ResultFinalizationRecord {
  studentRef: string;
  resultFingerprint: string;
  finalizedAt: string;
}

export interface LmsUploadStudentResult {
  studentRef: string;
  resultFingerprint: string;
  status: LmsUploadStudentStatus;
  sanitizedError?: string | null;
}

export interface LmsUploadAttemptResult {
  attemptId: string;
  mode: LmsUploadMode;
  provider: string;
  courseId: string;
  assignmentId: string;
  startedAt: string;
  finishedAt: string;
  attemptedCount: number;
  successCount: number;
  failureCount: number;
  studentResults: LmsUploadStudentResult[];
}

export interface ResultsLmsState {
  selectedTarget?: ResultsLmsTarget | null;
  finalizationRecords: ResultFinalizationRecord[];
  uploadAttempts: LmsUploadAttemptResult[];
}

export interface ResultsQuestionMetric {
  questionId: string;
  questionNumber: number;
  maxPoints?: number | null;
  reviewed: boolean;
  sampleSize: number;
  averagePoints?: number | null;
  averagePercent?: number | null;
  difficultyPercent?: number | null;
}

export interface ResultsExamMetrics {
  scoredStudentCount: number;
  averageScore?: number | null;
  medianScore?: number | null;
  minScore?: number | null;
  maxScore?: number | null;
  questionMetrics: ResultsQuestionMetric[];
}

export interface ResultsLmsReviewSummary {
  totalReviewableQuestions: number;
  unreviewedQuestionCount: number;
  hasUnreviewedQuestions: boolean;
}

export interface ResultQuestionRow {
  questionId: string;
  questionNumber: number;
  maxPoints?: number | null;
  effectiveTotalPoints?: number | null;
  effectiveFeedbackText: string;
  usesModeratedTotal: boolean;
  usesModeratedFeedback: boolean;
  blockedReason?: string | null;
}

export interface ResultStudentRow {
  studentRef: string;
  aggregateTotal: number;
  aggregateComplete: boolean;
  readyToFinalize: boolean;
  blockedReasons: string[];
  questionRows: ResultQuestionRow[];
  resultFingerprint?: string | null;
  finalized: boolean;
  staleFinalization: boolean;
  finalizedAt?: string | null;
  uploaded: boolean;
  uploadFailed: boolean;
  latestUploadError?: string | null;
  lastUploadAttemptId?: string | null;
}

export interface ResultsLmsUploadResponse {
  attempt: LmsUploadAttemptResult;
  workspace: ExamWorkspaceState;
}

export interface ResultsLmsReportPreview {
  studentRef: string;
  resultFingerprint?: string | null;
  html: string;
}

export interface StudentWorkflowSubmission {
  studentRef: string;
  canonicalPdfPath: string;
  pageCount: number;
  stage: string;
  latestJobId: string | null;
  failureMessage: string | null;
  warnings: WorkspaceWarning[];
  pageArtifacts: StudentWorkflowPage[];
  alignmentPages: StudentWorkflowAlignmentPage[];
  detectReview?: StudentWorkflowDetectReview | null;
  answers: StudentWorkflowAnswer[];
}

export interface StudentWorkflowDetectRegion {
  x: number;
  y: number;
  width: number;
  height: number;
  units: 'rendered_page_pixels' | (string & {});
}

export interface StudentWorkflowDetectReviewRow {
  questionId: string;
  pageNumber: number;
  sourcePageImagePath: string;
  templateRegion: StudentWorkflowDetectRegion;
  warnings: WorkspaceWarning[];
  resolvedRegion?: StudentWorkflowDetectRegion | null;
}

export interface StudentWorkflowDetectReview {
  pendingRows: StudentWorkflowDetectReviewRow[];
  trustedCropTargets: unknown[];
}

export interface StudentWorkflowDetectReviewResolutionInput {
  questionId: string;
  pageNumber: number;
  region: StudentWorkflowDetectRegion;
}

export interface StudentWorkflowState {
  status: string;
  latestJobId: string | null;
  submissions: StudentWorkflowSubmission[];
}

export interface SaveCriterionScoreInput {
  studentRef: string;
  questionId: string;
  criterionIndex: number;
  pointsAwarded: number;
}

export interface DeleteStudentSubmissionInput {
  studentRef: string;
}

export interface SaveModeratedScoreInput {
  studentRef: string;
  questionId: string;
  moderatedTotalPoints: number;
}

export interface SaveModeratedFeedbackInput {
  studentRef: string;
  questionId: string;
  feedbackText: string;
}

export interface SaveResultsLmsAssignmentInput {
  assignmentId: string | null;
}

export interface SetSubmissionResultFinalizedInput {
  studentRef: string;
  finalized: boolean;
}

export interface FinalizeReadyResultsInput {
  studentRefs: string[];
}

export interface RunResultsLmsUploadInput {
  mode: LmsUploadMode;
  studentRefs: string[];
}

export interface RetryResultsLmsUploadInput {
  attemptId: string;
}

export type ResultsExportFormat = 'html_zip' | 'csv';

export interface RunResultsExportInput {
  format: ResultsExportFormat;
  studentRefs: string[];
  destinationPath: string;
}

export interface ResultsExportResponse {
  format: ResultsExportFormat;
  destinationPath: string;
  exportedCount: number;
}

export interface SetModerationQuestionReviewedInput {
  questionId: string;
  reviewed: boolean;
}

export function questionAnalysisStatusKind(
  state: QuestionAnalysisState | null | undefined
): 'idle' | 'inProgress' | 'complete' | 'failed' | 'other' {
  switch (state?.status ?? 'not_started') {
    case 'not_started':
      return 'idle';
    case 'starting':
    case 'running':
    case 'queued':
    case 'submitted':
    case 'in_progress':
    case 'processing':
      return 'inProgress';
    case 'ok':
      return 'complete';
    case 'error':
      return 'failed';
    default:
      return 'other';
  }
}

export function questionAnalysisIsComplete(
  state: QuestionAnalysisState | null | undefined
): boolean {
  return questionAnalysisStatusKind(state) === 'complete';
}

export function questionAnalysisIsInProgress(
  state: QuestionAnalysisState | null | undefined
): boolean {
  return questionAnalysisStatusKind(state) === 'inProgress';
}

export interface JobTraceEvent {
  sequence: number;
  eventType: string;
  progress: Record<string, unknown> | null;
  scope: Record<string, unknown> | null;
  data: Record<string, unknown>;
  createdAt: string;
}

export interface JobTraceSummary {
  jobId: string;
  commandName: string;
  state: string;
  submittedAt: string;
  startedAt: string | null;
  finishedAt: string | null;
  eventCount: number;
  studentRefs?: string[];
}

export interface JobTraceState {
  jobId: string;
  commandName: string;
  state: string;
  submittedAt: string;
  startedAt: string | null;
  finishedAt: string | null;
  studentRefs?: string[];
  request: Record<string, unknown>;
  result: Record<string, unknown> | null;
  error: Record<string, unknown> | null;
  events: JobTraceEvent[];
}

export interface RubricUpdateInput {
  questionId: string;
  criteria: RubricCriterion[];
  approve: boolean;
  rubricEditImpact?: 'minor' | 'grading' | null;
}

export interface StudentIntakeInput {
  studentRef: string;
  localStudentName?: string | null;
  rawPdfPath: string;
  desiredPageOrder?: number[];
  redactionRegionsPx?: StudentIntakeRedactionRegionInput[];
  rasterSizesByPage?: Record<number, StudentIntakeRasterSize>;
}

/** Passed to `onFinalizeSubmission`: LMS projects resolve a binding first; local-only projects pass an instructor-entered name directly into `run_student_intake`. */
export interface StudentIntakeFinalizePayload {
  rawPdfPath: string;
  courseId?: string | null;
  canvasUserId?: string | null;
  localStudentName?: string | null;
  desiredPageOrder: number[];
  redactionRegionsPx: StudentIntakeRedactionRegionInput[];
  rasterSizesByPage: Record<number, StudentIntakeRasterSize>;
}

export interface StudentIntakeFinalizeResult {
  workspaceState: ExamWorkspaceState;
  studentRef: string;
  bindingTokenHex: string | null;
}
