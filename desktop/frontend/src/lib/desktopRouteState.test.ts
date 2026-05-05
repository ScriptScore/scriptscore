// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import {
  analysisActiveState,
  analysisEventIsTerminal,
  hasRunnableStudentWorkflowRows,
  moderationNeedsAttention,
  resultsNeedAttention,
  runtimeEventQuestionId,
  runtimeJobKey,
  setStudentReviewSaveBusyState,
  studentReviewSaveKey
} from './desktopRouteState';
import type { ExamWorkspaceState, RuntimeJobEvent } from './types';

function workspace(overrides: Partial<ExamWorkspaceState> = {}): ExamWorkspaceState {
  return {
    project: {
      projectId: 'project_1',
      displayName: 'Midterm',
      subject: 'CS',
      courseCode: 'CS145',
      lmsCourseId: 'course_1',
      projectPath: '/tmp/project',
      createdAt: '1',
      updatedAt: '1'
    },
    status: 'approved',
    statusLabel: 'Approved',
    failureMessage: null,
    templatePreviewArtifacts: [],
    questions: [],
    redactionRegions: [],
    warnings: [],
    canApprove: true,
    canApproveRubric: true,
    ...overrides
  };
}

function runtimeEvent(
  eventType: RuntimeJobEvent['eventType'],
  payload: Record<string, unknown> = {},
  ids: Partial<Pick<RuntimeJobEvent, 'jobId' | 'requestId'>> = {}
): RuntimeJobEvent {
  return {
    eventType,
    commandName: 'exam.analyze',
    workerStatus: 'busy',
    requestId: 'requestId' in ids ? ids.requestId ?? null : 'request_1',
    jobId: 'jobId' in ids ? ids.jobId ?? null : 'job_1',
    payload
  };
}

describe('desktopRouteState', () => {
  it('detects moderation attention only for eligible unreviewed questions', () => {
    const base = workspace({
      studentWorkflow: {
        status: 'ready',
        latestJobId: null,
        submissions: [
          {
            studentRef: 'student_1',
            canonicalPdfPath: '/tmp/student.pdf',
            pageCount: 1,
            stage: 'graded',
            latestJobId: null,
            failureMessage: null,
            warnings: [],
            pageArtifacts: [],
            alignmentPages: [],
            answers: [
              {
                questionId: 'question_1',
                questionNumber: 1,
                cropImagePath: '/tmp/crop.png',
                moderationEligible: true,
                parseStatus: 'ok',
                parseConfidence: null,
                parseConfidenceSource: null,
                rawParsedText: null,
                verifiedText: null,
                reviewRequired: false,
                verified: true,
                stale: false,
                gradingStatus: 'graded',
                gradingConfidence: null,
                gradingConfidenceReason: null,
                questionMaxPoints: 5,
                totalPointsAwarded: 4,
                feedbackText: null,
                criterionResults: [],
                highlights: [],
                warnings: []
              }
            ]
          }
        ]
      },
      moderationState: {
        scoreOverrides: [],
        feedbackOverrides: [],
        questionReviews: []
      }
    });

    expect(moderationNeedsAttention(base)).toBe(true);
    expect(
      moderationNeedsAttention({
        ...base,
        moderationState: {
          scoreOverrides: [],
          feedbackOverrides: [],
          questionReviews: [{ questionId: 'question_1', reviewedAt: 'now' }]
        }
      })
    ).toBe(false);
    expect(moderationNeedsAttention(workspace())).toBe(false);
  });

  it('detects results attention for stale, failed, or ready unfinalized rows', () => {
    expect(
      resultsNeedAttention(
        workspace({
          resultsLmsRows: [
            {
              studentRef: 'student_1',
              aggregateTotal: 10,
              aggregateComplete: true,
              readyToFinalize: true,
              blockedReasons: [],
              questionRows: [],
              finalized: false,
              staleFinalization: false,
              uploaded: false,
              uploadFailed: false
            }
          ]
        })
      )
    ).toBe(true);
    expect(
      resultsNeedAttention(
        workspace({
          resultsLmsRows: [
            {
              studentRef: 'student_1',
              aggregateTotal: 10,
              aggregateComplete: true,
              readyToFinalize: false,
              blockedReasons: [],
              questionRows: [],
              finalized: true,
              staleFinalization: true,
              uploaded: false,
              uploadFailed: false
            },
            {
              studentRef: 'student_2',
              aggregateTotal: 8,
              aggregateComplete: true,
              readyToFinalize: false,
              blockedReasons: [],
              questionRows: [],
              finalized: true,
              staleFinalization: false,
              uploaded: false,
              uploadFailed: true
            }
          ]
        })
      )
    ).toBe(true);
    expect(resultsNeedAttention(workspace({ resultsLmsRows: [] }))).toBe(false);
  });

  it('maps runtime event ids, question ids, active states, and terminal states', () => {
    expect(runtimeJobKey(runtimeEvent('job_started', {}, { jobId: 'job_2' }))).toBe('job_2');
    expect(runtimeJobKey(runtimeEvent('job_started', {}, { jobId: null, requestId: 'request_2' }))).toBe(
      'request_2'
    );
    expect(runtimeEventQuestionId(runtimeEvent('job_progress', { questionId: 'question_1' }))).toBe(
      'question_1'
    );
    expect(runtimeEventQuestionId(runtimeEvent('job_progress', { question_id: 'question_2' }))).toBe(
      'question_2'
    );
    expect(runtimeEventQuestionId(runtimeEvent('job_progress', { questionId: '   ' }))).toBeNull();
    expect(analysisActiveState(runtimeEvent('job_queued'))).toBe('queued');
    expect(analysisActiveState(runtimeEvent('job_started'))).toBe('running');
    expect(analysisActiveState(runtimeEvent('job_finished'))).toBeNull();
    expect(analysisEventIsTerminal(runtimeEvent('job_failed'))).toBe(true);
    expect(analysisEventIsTerminal(runtimeEvent('job_cancelled'))).toBe(true);
    expect(analysisEventIsTerminal(runtimeEvent('job_progress'))).toBe(false);
  });

  it('detects runnable student workflow stages', () => {
    const state = workspace({
      studentWorkflow: {
        status: 'ready',
        latestJobId: null,
        submissions: [
          {
            studentRef: 'student_1',
            canonicalPdfPath: '/tmp/student.pdf',
            pageCount: 1,
            stage: 'uploaded',
            latestJobId: null,
            failureMessage: null,
            warnings: [],
            pageArtifacts: [],
            alignmentPages: [],
            answers: []
          },
          {
            studentRef: 'student_2',
            canonicalPdfPath: '/tmp/student-2.pdf',
            pageCount: 1,
            stage: 'parse',
            latestJobId: null,
            failureMessage: null,
            warnings: [],
            pageArtifacts: [],
            alignmentPages: [],
            answers: []
          }
        ]
      }
    });
    expect(hasRunnableStudentWorkflowRows(state)).toBe(true);
    expect(
      hasRunnableStudentWorkflowRows({
        ...state,
        studentWorkflow: {
          status: 'done',
          latestJobId: null,
          submissions: [
            {
              ...state.studentWorkflow!.submissions[0],
              stage: 'uploaded'
            }
          ]
        }
      })
    ).toBe(false);
    expect(hasRunnableStudentWorkflowRows(null)).toBe(false);
  });

  it('builds student review save keys and immutable busy maps', () => {
    const key = studentReviewSaveKey('criterion', 'student_1', 'question_2', 3);
    expect(key).toBe('criterion:student_1:question_2:3');

    const original = { other: true };
    const busy = setStudentReviewSaveBusyState(original, key, true);
    expect(original).toEqual({ other: true });
    expect(busy).toEqual({ other: true, [key]: true });

    const cleared = setStudentReviewSaveBusyState(busy, key, false);
    expect(cleared).toEqual({ other: true });
  });
});
