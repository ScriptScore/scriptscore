// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import type { ExamWorkspaceState } from '$lib/types';
import { studentIntakePrerequisitesMet } from './studentIntakePrerequisites';
import { baseWorkspaceState } from '../test/studentTestFixtures';

const approvedCriterion = {
  criterionId: 'criterion_1',
  label: 'Accuracy',
  points: 5,
  partialCreditGuidance: '',
  source: 'manual'
};

function readyWorkspace(overrides: Partial<ExamWorkspaceState> = {}): ExamWorkspaceState {
  const workspace = baseWorkspaceState();
  const ready = {
    ...workspace,
    questions: workspace.questions.map((question) => ({
      ...question,
      rubric: {
        status: 'approved',
        criteria: [approvedCriterion],
        warnings: [],
        approvedAt: '2026-04-29T00:00:00Z',
        latestJobId: null,
        approvalBasis: {
          questionText: question.text,
          questionContext: question.analysis?.questionContext ?? '',
          maxPoints: question.maxPoints,
          criteria: [approvedCriterion]
        }
      }
    }))
  };
  return { ...ready, ...overrides };
}

describe('studentIntakePrerequisitesMet', () => {
  it('stays true after workflow stage advances beyond initial intake readiness', () => {
    expect(
      studentIntakePrerequisitesMet(
        readyWorkspace({
          workflowStage: 'results_upload_ready',
          workflowLabel: 'Ready for Results'
        })
      )
    ).toBe(true);
  });

  it('requires approved rubrics and completed analysis', () => {
    const workspace = readyWorkspace({
      questions: [
        {
          ...baseWorkspaceState().questions[0]!,
          analysis: { ...baseWorkspaceState().questions[0]!.analysis!, status: 'queued' },
          rubric: {
            status: 'draft',
            criteria: [],
            warnings: [],
            approvedAt: null,
            latestJobId: null
          }
        }
      ]
    });

    expect(studentIntakePrerequisitesMet(workspace)).toBe(false);
  });

  it('does not treat approvedAt as sufficient when rubric status is not approved', () => {
    const workspace = readyWorkspace({
      questions: [
        {
          ...baseWorkspaceState().questions[0]!,
          rubric: {
            status: 'draft',
            criteria: [approvedCriterion],
            warnings: [],
            approvedAt: '2026-04-29T00:00:00Z',
            latestJobId: null,
            approvalBasis: {
              questionText: 'Question text',
              questionContext: '',
              maxPoints: 5,
              criteria: [approvedCriterion]
            }
          }
        }
      ]
    });

    expect(studentIntakePrerequisitesMet(workspace)).toBe(false);
  });

  it('requires approved rubrics to match the current approval basis', () => {
    const workspace = readyWorkspace({
      questions: [
        {
          ...baseWorkspaceState().questions[0]!,
          rubric: {
            status: 'approved',
            criteria: [approvedCriterion],
            warnings: [],
            approvedAt: '2026-04-29T00:00:00Z',
            latestJobId: null,
            approvalBasis: {
              questionText: 'Outdated question text',
              questionContext: '',
              maxPoints: 5,
              criteria: [approvedCriterion]
            }
          }
        }
      ]
    });

    expect(studentIntakePrerequisitesMet(workspace)).toBe(false);
  });

  it('accepts approval basis text that matches completed clean analysis text', () => {
    const workspace = readyWorkspace({
      questions: [
        {
          ...baseWorkspaceState().questions[0]!,
          text: '1. Question text',
          analysis: {
            ...baseWorkspaceState().questions[0]!.analysis!,
            status: 'ok',
            questionTextClean: 'Question text'
          },
          rubric: {
            status: 'approved',
            criteria: [approvedCriterion],
            warnings: [],
            approvedAt: '2026-04-29T00:00:00Z',
            latestJobId: null,
            approvalBasis: {
              questionText: 'Question text',
              questionContext: '',
              maxPoints: 5,
              criteria: [approvedCriterion]
            }
          }
        }
      ]
    });

    expect(studentIntakePrerequisitesMet(workspace)).toBe(true);
  });

  it('requires redaction when project redaction is enabled', () => {
    expect(
      studentIntakePrerequisitesMet(
        readyWorkspace({
          redactionRegions: []
        })
      )
    ).toBe(false);
  });

  it('accepts acknowledged redaction skip as satisfying intake redaction readiness', () => {
    expect(
      studentIntakePrerequisitesMet(
        readyWorkspace({
          redactionRegions: [],
          warnings: [
            {
              code: 'redaction_skipped',
              message: 'Redaction was skipped.',
              scope: null
            }
          ]
        })
      )
    ).toBe(true);
  });
});
