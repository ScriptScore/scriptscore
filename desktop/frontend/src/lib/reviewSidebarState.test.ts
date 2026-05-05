// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import { reviewSidebarIconState } from '$lib/reviewSidebarState';
import type { QuestionRecord, RubricCriterion, WorkspaceWarning } from '$lib/types';

function criterion(points: number): RubricCriterion {
  return {
    criterionId: `criterion_${points}`,
    label: 'Correctness',
    points,
    partialCreditGuidance: 'Award credit.',
    source: 'manual'
  };
}

function warning(code: string): WorkspaceWarning {
  return {
    code,
    message: 'Warning',
    scope: null
  };
}

function question(overrides: Partial<QuestionRecord> = {}): QuestionRecord {
  return {
    questionId: 'question_1',
    questionNumber: 1,
    pageNumber: 1,
    maxPoints: 5,
    text: 'Question text',
    baselinePdfText: 'Question text',
    sourceArtifactId: null,
    analysis: {
      status: 'ok',
      questionTextClean: 'Question text',
      questionContext: '',
      warnings: [],
      latestJobId: 'job_analyze_1'
    },
    rubric: {
      status: 'draft',
      criteria: [criterion(5)],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    },
    ...overrides
  };
}

describe('reviewSidebarIconState', () => {
  it('shows a warning for empty saved rubric criteria when the question has points', () => {
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'draft',
            criteria: [],
            warnings: [],
            approvedAt: null,
            latestJobId: 'job_rubric_1'
          }
        })
      })
    ).toBe('rubric-warning');
  });

  it('shows a warning for selected draft criteria that do not add up to max points', () => {
    expect(
      reviewSidebarIconState({
        question: question(),
        criteriaOverride: [criterion(3)]
      })
    ).toBe('rubric-warning');
  });

  it('derives point mismatch warnings from the current rubric criteria', () => {
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'draft',
            criteria: [criterion(5)],
            warnings: [warning('rubric_points_mismatch')],
            approvedAt: null,
            latestJobId: 'job_rubric_1'
          }
        })
      })
    ).toBe('none');

    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'draft',
            criteria: [criterion(4)],
            warnings: [warning('rubric_points_mismatch')],
            approvedAt: null,
            latestJobId: 'job_rubric_1'
          }
        })
      })
    ).toBe('rubric-warning');
  });

  it('shows approved only when the rubric has no warning or error state', () => {
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'approved',
            criteria: [criterion(5)],
            warnings: [],
            approvedAt: '2026-04-08T00:00:00Z',
            latestJobId: 'job_rubric_1'
          }
        })
      })
    ).toBe('rubric-approved');
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'approved',
            criteria: [],
            warnings: [],
            approvedAt: '2026-04-08T00:00:00Z',
            latestJobId: 'job_rubric_1'
          }
        })
      })
    ).toBe('rubric-warning');
  });

  it('prioritizes persisted rubric errors and warnings ahead of approved state', () => {
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'approved',
            criteria: [criterion(5)],
            warnings: [warning('rubric_semantic_review_failed')],
            approvedAt: '2026-04-08T00:00:00Z',
            latestJobId: 'job_rubric_1'
          }
        })
      })
    ).toBe('rubric-warning');
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'approved',
            criteria: [criterion(5)],
            warnings: [warning('rubric_generate_failed')],
            approvedAt: '2026-04-08T00:00:00Z',
            latestJobId: 'job_rubric_1'
          }
        })
      })
    ).toBe('rubric-error');
  });

  it('prioritizes active jobs over rubric error, warning, and approved states', () => {
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'error',
            criteria: [],
            warnings: [],
            approvedAt: '2026-04-08T00:00:00Z',
            latestJobId: 'job_rubric_1'
          }
        }),
        rubricJobState: 'queued'
      })
    ).toBe('rubric-queued');
    expect(
      reviewSidebarIconState({
        question: question({
          rubric: {
            status: 'approved',
            criteria: [],
            warnings: [],
            approvedAt: '2026-04-08T00:00:00Z',
            latestJobId: 'job_rubric_1'
          }
        }),
        rubricJobState: 'running'
      })
    ).toBe('rubric-running');
  });

  it('prioritizes analysis states over rubric states', () => {
    expect(
      reviewSidebarIconState({
        question: question({
          analysis: {
            status: 'queued',
            questionTextClean: null,
            questionContext: null,
            warnings: [],
            latestJobId: 'job_analyze_1'
          }
        }),
        rubricJobState: 'running'
      })
    ).toBe('analysis-queued');
    expect(
      reviewSidebarIconState({
        question: question(),
        analysisInProgress: true,
        rubricJobState: 'running'
      })
    ).toBe('rubric-running');
    expect(
      reviewSidebarIconState({
        question: question({
          analysis: {
            status: 'ok',
            questionTextClean: 'Question text',
            questionContext: '',
            warnings: [],
            latestJobId: 'job_analyze_1'
          }
        }),
        analysisJobState: 'running',
        rubricJobState: 'running'
      })
    ).toBe('analysis-running');
  });

  it('does not show queued for an auto-rubric eligible question without an observed job', () => {
    expect(
      reviewSidebarIconState({
        question: question({
          imagePath: '/tmp/q1.png',
          rubric: {
            status: 'not_started',
            criteria: [],
            warnings: [],
            approvedAt: null,
            latestJobId: null
          }
        })
      })
    ).toBe('none');
  });
});
