// SPDX-License-Identifier: AGPL-3.0-only
import type { AnalysisJobState } from '$lib/reviewSidebarState';
import type { ExamWorkspaceState, RuntimeJobEvent } from '$lib/types';

const runnableStudentWorkflowStages = new Set([
  '',
  'intake_ready',
  'stopped',
  'failed',
  'alignment',
  'canonicalize',
  'detect',
  'crop',
  'pii',
  'parse',
  'grading'
]);

export function moderationNeedsAttention(workspace: ExamWorkspaceState): boolean {
  const eligibleQuestionIds = new Set(
    (workspace.studentWorkflow?.submissions ?? [])
      .flatMap((submission) => submission.answers ?? [])
      .filter((answer) => answer.moderationEligible)
      .map((answer) => answer.questionId)
  );
  if (eligibleQuestionIds.size === 0) {
    return false;
  }
  const reviewedQuestionIds = new Set(
    (workspace.moderationState?.questionReviews ?? []).map((review) => review.questionId)
  );
  return [...eligibleQuestionIds].some((questionId) => !reviewedQuestionIds.has(questionId));
}

export function resultsNeedAttention(workspace: ExamWorkspaceState): boolean {
  return (workspace.resultsLmsRows ?? []).some(
    (row) => row.staleFinalization || row.uploadFailed || (row.readyToFinalize && !row.finalized)
  );
}

export function runtimeJobKey(event: RuntimeJobEvent): string | null {
  return event.jobId ?? event.requestId ?? null;
}

export function runtimeEventQuestionId(event: RuntimeJobEvent): string | null {
  const raw = event.payload.questionId ?? event.payload.question_id;
  return typeof raw === 'string' && raw.trim().length > 0 ? raw : null;
}

export function analysisActiveState(event: RuntimeJobEvent): AnalysisJobState | null {
  if (event.eventType === 'job_queued') {
    return 'queued';
  }
  if (
    event.eventType === 'job_submitted' ||
    event.eventType === 'job_started' ||
    event.eventType === 'job_progress'
  ) {
    return 'running';
  }
  return null;
}

export function analysisEventIsTerminal(event: RuntimeJobEvent): boolean {
  return (
    event.eventType === 'job_finished' ||
    event.eventType === 'job_failed' ||
    event.eventType === 'job_cancelled'
  );
}

export function hasRunnableStudentWorkflowRows(state: ExamWorkspaceState | null): boolean {
  return (state?.studentWorkflow?.submissions ?? []).some((submission) =>
    runnableStudentWorkflowStages.has(submission.stage)
  );
}

export function studentReviewSaveKey(kind: string, ...parts: Array<string | number>): string {
  return [kind, ...parts].join(':');
}

export function setStudentReviewSaveBusyState(
  current: Record<string, boolean>,
  key: string,
  busy: boolean
): Record<string, boolean> {
  const next = { ...current };
  if (busy) {
    next[key] = true;
  } else {
    delete next[key];
  }
  return next;
}
