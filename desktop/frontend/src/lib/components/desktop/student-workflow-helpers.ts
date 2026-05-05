// SPDX-License-Identifier: AGPL-3.0-only
import type { FeedbackTone } from './ui';
import type { StudentIntakeSummary, StudentWorkflowSubmission, StudentWorkflowHighlightSpan } from '$lib/types';

export const draftGradingReadyLabel = 'draft grading ready';
export const waitingStageLabel = 'waiting';

export type StageView = 'submissionPages' | 'alignmentReview' | 'detectReview' | 'questionDetail';
export type ProgressTone = 'info' | 'success' | 'warning' | 'error' | 'muted';

export interface StageProgressRange {
  start: number;
  end: number;
}

export interface WorkflowCommandProgressState {
  totalStages: number;
  currentStage: number;
}

export interface WorkflowProgressMetrics {
  criterionCount: number;
  questionCount: number;
}

export interface SidebarStudentEntry {
  studentRef: string;
  displayName: string;
  dotClass: string;
  label: string;
  debugLine: string;
  statusGroup:
    | 'needsReview'
    | 'processing'
    | 'ready'
    | 'graded'
    | 'failedStopped'
    | 'noSubmission';
  progress: number;
}

export function stageLabel(stage: string): string {
  switch (stage) {
    case 'intake_ready':
      return waitingStageLabel;
    case 'stopped':
      return 'stopped';
    case 'alignment':
      return 'aligning';
    case 'alignment_review':
      return 'alignment review';
    case 'canonicalize':
      return 'canonicalizing';
    case 'transform':
      return 'transforming';
    case 'detect':
      return 'detecting';
    case 'detect_review':
      return 'region review';
    case 'crop':
      return 'cropping';
    case 'pii':
      return 'screening PII';
    case 'parse':
      return 'parsing';
    case 'parse_review':
      return 'parse review';
    case 'grading':
      return 'grading';
    case 'manual_grading':
      return 'manual grading';
    case 'graded':
      return draftGradingReadyLabel;
    case 'failed':
      return 'failed';
    default:
      return stage || waitingStageLabel;
  }
}

export function stageProgressRange(stage: string): StageProgressRange {
  switch (stage) {
    case 'stopped':
      return { start: 0, end: 0 };
    case 'alignment':
      return { start: 0, end: 12 };
    case 'alignment_review':
      return { start: 12, end: 12 };
    case 'canonicalize':
      return { start: 12, end: 25 };
    case 'transform':
      return { start: 12, end: 25 };
    case 'detect':
      return { start: 25, end: 35 };
    case 'detect_review':
      return { start: 35, end: 35 };
    case 'crop':
      return { start: 35, end: 45 };
    case 'pii':
      return { start: 45, end: 55 };
    case 'parse':
      return { start: 55, end: 70 };
    case 'parse_review':
      return { start: 70, end: 70 };
    case 'grading':
      return { start: 70, end: 99 };
    case 'manual_grading':
      return { start: 99, end: 99 };
    case 'graded':
      return { start: 100, end: 100 };
    default:
      return { start: 0, end: 0 };
  }
}

export function stageProgressValue(stage: string): number {
  return stageProgressRange(stage).start;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function gradingCommandProgressRanges(
  metrics: WorkflowProgressMetrics | null
): Record<'preliminary' | 'feedback' | 'markup', StageProgressRange> {
  const gradingRange = stageProgressRange('grading');
  const criterionCount =
    metrics && Number.isFinite(metrics.criterionCount) && metrics.criterionCount > 0
      ? metrics.criterionCount
      : 1;
  const questionCount =
    metrics && Number.isFinite(metrics.questionCount) && metrics.questionCount > 0
      ? metrics.questionCount
      : 1;
  const totalUnits = criterionCount + questionCount + questionCount;
  const preliminaryEnd =
    gradingRange.start + ((gradingRange.end - gradingRange.start) * criterionCount) / totalUnits;
  const feedbackEnd =
    preliminaryEnd + ((gradingRange.end - gradingRange.start) * questionCount) / totalUnits;
  return {
    preliminary: { start: gradingRange.start, end: preliminaryEnd },
    feedback: { start: preliminaryEnd, end: feedbackEnd },
    markup: { start: feedbackEnd, end: gradingRange.end }
  };
}

export function commandProgressRange(
  commandName: string,
  metrics: WorkflowProgressMetrics | null = null
): StageProgressRange | null {
  switch (commandName) {
    case 'scans.align-auto':
      return { start: 0, end: 12 };
    case 'scans.canonicalize':
      return { start: 12, end: 25 };
    case 'scans.transform':
      return { start: 12, end: 25 };
    case 'scans.detect':
      return { start: 25, end: 35 };
    case 'scans.crop':
      return { start: 35, end: 45 };
    case 'scans.pii':
      return { start: 45, end: 55 };
    case 'scans.parse':
      return { start: 55, end: 70 };
    case 'grading.score-preliminary':
      return gradingCommandProgressRanges(metrics).preliminary;
    case 'grading.draft-feedback':
      return gradingCommandProgressRanges(metrics).feedback;
    case 'grading.markup':
      return gradingCommandProgressRanges(metrics).markup;
    default:
      return null;
  }
}

export function commandWorkflowStage(commandName: string): string | null {
  switch (commandName) {
    case 'scans.align-auto':
      return 'alignment';
    case 'scans.canonicalize':
      return 'canonicalize';
    case 'scans.transform':
      return 'transform';
    case 'scans.detect':
      return 'detect';
    case 'scans.crop':
      return 'crop';
    case 'scans.pii':
      return 'pii';
    case 'scans.parse':
      return 'parse';
    case 'grading.score-preliminary':
    case 'grading.draft-feedback':
    case 'grading.markup':
      return 'grading';
    default:
      return null;
  }
}

export function stageProgressTone(stage: string): ProgressTone {
  if (stage === 'graded') {
    return 'success';
  }
  if (stage === 'alignment_review' || stage === 'detect_review' || stage === 'parse_review' || stage === 'manual_grading') {
    return 'warning';
  }
  if (stage === 'failed') {
    return 'error';
  }
  if (stage !== 'intake_ready' && stage !== 'stopped') {
    return 'info';
  }
  return 'muted';
}

export function interpolateProgress(range: StageProgressRange, percent: number): number {
  const clampedPercent = Math.max(0, Math.min(100, percent));
  return range.start + ((range.end - range.start) * clampedPercent) / 100;
}

export function updateWorkflowCommandProgressState(
  current: WorkflowCommandProgressState | null,
  payload: Record<string, unknown>
): WorkflowCommandProgressState {
  const next: WorkflowCommandProgressState = current
    ? { ...current }
    : { totalStages: 1, currentStage: 1 };
  const innerEvent = payload.event;
  const data = isRecord(payload.data) ? payload.data : null;
  if (innerEvent === 'started' && data) {
    const totalStages = data.total_stages;
    if (typeof totalStages === 'number' && Number.isFinite(totalStages) && totalStages > 0) {
      next.totalStages = totalStages;
      next.currentStage = 1;
    }
  }
  if (innerEvent === 'stage_started' && data) {
    const stageNumber = data.stage_number;
    if (typeof stageNumber === 'number' && Number.isFinite(stageNumber) && stageNumber > 0) {
      next.currentStage = Math.min(stageNumber, Math.max(1, next.totalStages));
    }
  }
  return next;
}

export function normalizedWorkflowCommandPercent(
  state: WorkflowCommandProgressState,
  percent: number
): number {
  const clampedPercent = Math.max(0, Math.min(100, percent));
  const totalStages = Math.max(1, state.totalStages);
  const currentStage = Math.min(Math.max(1, state.currentStage), totalStages);
  return (((currentStage - 1) + clampedPercent / 100) / totalStages) * 100;
}

/** Matches CLI `progress(completed, total)` percent truncation when `percent` is omitted. */
export function readCliProgressPercent(payload: Record<string, unknown>): number | null {
  const progress = payload.progress;
  if (!isRecord(progress)) {
    return null;
  }
  const percent = progress.percent;
  if (typeof percent === 'number' && Number.isFinite(percent)) {
    return percent;
  }
  const completed = progress.completed;
  const total = progress.total;
  if (
    typeof completed === 'number' &&
    typeof total === 'number' &&
    Number.isFinite(completed) &&
    Number.isFinite(total) &&
    total > 0
  ) {
    return Math.trunc((completed / total) * 100);
  }
  return null;
}

/** True when the envelope advances CLI substages without a progress summary (e.g. scans.parse `stage_started`). */
export function isSubstageBoundaryProgressPayload(
  payload: Record<string, unknown>,
  state: WorkflowCommandProgressState
): boolean {
  return state.totalStages > 1 && payload.event === 'stage_started' && readCliProgressPercent(payload) === null;
}

export function workflowBarValueFromCommandProgress(
  range: StageProgressRange,
  state: WorkflowCommandProgressState,
  innerPercent: number
): number {
  return interpolateProgress(range, normalizedWorkflowCommandPercent(state, innerPercent));
}

export function confidenceLabel(confidence: string | null | undefined): string {
  switch (confidence) {
    case 'high':
      return 'high confidence';
    case 'medium':
      return 'medium confidence';
    case 'low':
      return 'low confidence';
    default:
      return 'confidence unavailable';
  }
}

export function confidenceBadgeTone(confidence: string | null | undefined): FeedbackTone {
  switch (confidence) {
    case 'high':
      return 'success';
    case 'medium':
      return 'info';
    case 'low':
      return 'warning';
    default:
      return 'muted';
  }
}

export function stageViewForSubmission(
  submission: StudentWorkflowSubmission | null
): StageView {
  const stage = submission?.stage;
  if (!stage) return 'submissionPages';
  switch (stage) {
    case 'alignment_review':
      return 'alignmentReview';
    case 'detect_review':
      return 'detectReview';
    case 'canonicalize':
    case 'transform':
    case 'detect':
    case 'crop':
    case 'pii':
    case 'parse':
    case 'parse_review':
    case 'grading':
    case 'manual_grading':
    case 'graded':
    case 'failed':
      return 'questionDetail';
    case 'stopped':
      return 'submissionPages';
    default:
      return 'submissionPages';
  }
}

export function dotClassForState(
  item: StudentIntakeSummary | null,
  submission: StudentWorkflowSubmission | null
): string {
  if (!item) {
    return 'bg-workspace-dot-idle';
  }
  if (submission?.stage === 'failed' || item.ingestStatus === 'failed') {
    return 'bg-destructive';
  }
  if (
    submission?.stage === 'alignment_review' ||
    submission?.stage === 'detect_review' ||
    submission?.stage === 'parse_review' ||
    submission?.stage === 'manual_grading'
  ) {
    return 'bg-workspace-dot-pending';
  }
  if (submission?.stage === 'graded') {
    return 'bg-workspace-dot-complete';
  }
  if (submission && submission.stage !== 'intake_ready' && submission.stage !== 'stopped') {
    return 'bg-message-info-border';
  }
  if ((item.examPagePaths?.length ?? 0) > 0) {
    return 'bg-workspace-dot-complete';
  }
  return 'bg-workspace-dot-pending';
}

export function labelForState(
  item: StudentIntakeSummary | null,
  submission: StudentWorkflowSubmission | null
): string {
  if (!item) return 'no submission';
  if (submission) return stageLabel(submission.stage);
  if (item.ingestStatus === 'failed') return 'needs review';
  if ((item.examPagePaths?.length ?? 0) > 0) return 'canonical · ready';
  return 'processing';
}

export function basename(path: string): string {
  const parts = path.split('/');
  return parts[parts.length - 1] ?? path;
}

export function studentRefSortKey(studentRef: string): string {
  const match = /^student_(\d+)$/.exec(studentRef);
  const index = match ? Number(match[1]) : Number.MAX_SAFE_INTEGER;
  return `${index.toString().padStart(12, '0')}:${studentRef}`;
}

export function highlightKindToClass(kind: string): string {
  switch (kind) {
    case 'correct':
      return 'text-message-success-text bg-message-success-bg border-message-success-border';
    case 'incorrect':
      return 'text-message-error-text bg-message-error-bg border-message-error-border';
    default:
      return 'text-message-info-text bg-message-info-bg border-message-info-border';
  }
}

export interface MarkedTextSegment {
  text: string;
  kind: string | null;
}

export function markedTextSegments(
  text: string,
  highlights: StudentWorkflowHighlightSpan[]
): MarkedTextSegment[] {
  if (!highlights || highlights.length === 0) return [{ text, kind: null }];
  const sorted = [...highlights].sort((a, b) => a.startChar - b.startChar);
  const segments: MarkedTextSegment[] = [];
  let cursor = 0;
  for (const h of sorted) {
    const start = Math.max(0, Math.min(h.startChar, text.length));
    const end = Math.max(start, Math.min(h.endChar, text.length));
    if (start > cursor) {
      segments.push({ text: text.slice(cursor, start), kind: null });
    }
    if (end > start) {
      segments.push({ text: text.slice(start, end), kind: h.kind });
    }
    cursor = Math.max(cursor, end);
  }
  if (cursor < text.length) {
    segments.push({ text: text.slice(cursor), kind: null });
  }
  return segments;
}
