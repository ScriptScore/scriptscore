// SPDX-License-Identifier: AGPL-3.0-only
import {
  questionAnalysisStatusKind,
  type QuestionRecord,
  type RubricCriterion,
  type WorkspaceWarning
} from '$lib/types';

export type RubricGenerationJobState = 'queued' | 'running';
export type AnalysisJobState = 'queued' | 'running';

export type ReviewSidebarIconState =
  | 'analysis-running'
  | 'analysis-queued'
  | 'rubric-running'
  | 'rubric-queued'
  | 'rubric-error'
  | 'rubric-warning'
  | 'rubric-approved'
  | 'none';

const hardRubricWarningCodes = new Set([
  'rubric_generate_failed',
  'rubric_output_invalid',
  'rubric_invalid_output',
  'rubric_schema_invalid'
]);
const derivedRubricPointWarningCodes = new Set(['rubric_points_mismatch']);

function warningLooksLikeHardRubricError(warning: WorkspaceWarning): boolean {
  return warning.code != null && hardRubricWarningCodes.has(warning.code);
}

function warningIsDerivedFromRubricPoints(warning: WorkspaceWarning): boolean {
  return warning.code != null && derivedRubricPointWarningCodes.has(warning.code);
}

export function rubricCriteriaPointsWarning(
  question: QuestionRecord | undefined,
  criteria: RubricCriterion[]
): boolean {
  if (!question) {
    return false;
  }
  if (criteria.length === 0) {
    return question.maxPoints != null && question.maxPoints > 0;
  }
  if (question.maxPoints == null || question.maxPoints <= 0) {
    return true;
  }
  return criteria.reduce((sum, criterion) => sum + (criterion.points ?? 0), 0) !== question.maxPoints;
}

export function questionAnalysisRunning(
  question: QuestionRecord | undefined,
  analysisInProgress: boolean,
  analysisJobState?: AnalysisJobState | null
): boolean {
  if (analysisJobState) {
    return true;
  }
  if (questionAnalysisStatusKind(question?.analysis) === 'inProgress') {
    return true;
  }
  return (
    analysisInProgress &&
    questionAnalysisStatusKind(question?.analysis) !== 'complete' &&
    questionAnalysisStatusKind(question?.analysis) !== 'failed'
  );
}

export function questionAnalysisSidebarState(
  question: QuestionRecord | undefined,
  analysisInProgress: boolean,
  analysisJobState?: AnalysisJobState | null
): Extract<ReviewSidebarIconState, 'analysis-running' | 'analysis-queued'> | null {
  if (analysisJobState === 'queued') {
    return 'analysis-queued';
  }
  if (analysisJobState === 'running') {
    return 'analysis-running';
  }
  switch (question?.analysis?.status ?? 'not_started') {
    case 'queued':
      return 'analysis-queued';
    case 'submitted':
    case 'starting':
    case 'running':
    case 'in_progress':
    case 'processing':
      return 'analysis-running';
    default:
      break;
  }
  if (
    analysisInProgress &&
    questionAnalysisStatusKind(question?.analysis) !== 'complete' &&
    questionAnalysisStatusKind(question?.analysis) !== 'failed'
  ) {
    return 'analysis-running';
  }
  return null;
}

export function rubricSidebarState(
  question: QuestionRecord | undefined,
  criteriaOverride: RubricCriterion[] | null = null
): Extract<ReviewSidebarIconState, 'rubric-error' | 'rubric-warning' | 'rubric-approved'> | 'none' {
  const rubric = question?.rubric;
  if (!rubric) {
    return 'none';
  }
  if (rubric.status === 'error' || rubric.warnings.some(warningLooksLikeHardRubricError)) {
    return 'rubric-error';
  }
  if (rubric.warnings.some((warning) => !warningIsDerivedFromRubricPoints(warning))) {
    return 'rubric-warning';
  }
  const criteria = criteriaOverride ?? rubric.criteria;
  if (rubric.status !== 'not_started' && rubricCriteriaPointsWarning(question, criteria)) {
    return 'rubric-warning';
  }
  if (rubric.approvedAt != null) {
    return 'rubric-approved';
  }
  return 'none';
}

export function reviewSidebarIconState(input: {
  question: QuestionRecord | undefined;
  analysisJobState?: AnalysisJobState | null;
  rubricJobState?: RubricGenerationJobState | null;
  criteriaOverride?: RubricCriterion[] | null;
  analysisInProgress?: boolean;
}): ReviewSidebarIconState {
  const analysisState = questionAnalysisSidebarState(
    input.question,
    input.analysisInProgress ?? false,
    input.analysisJobState ?? null
  );
  if (analysisState) {
    return analysisState;
  }
  if (input.rubricJobState === 'running') {
    return 'rubric-running';
  }
  if (input.rubricJobState === 'queued') {
    return 'rubric-queued';
  }
  return rubricSidebarState(input.question, input.criteriaOverride ?? null);
}
