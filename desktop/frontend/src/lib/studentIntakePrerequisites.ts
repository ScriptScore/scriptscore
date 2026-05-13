// SPDX-License-Identifier: AGPL-3.0-only
import type { ExamWorkspaceState } from '$lib/types';

const REDACTION_SKIPPED_WARNING_CODE = 'redaction_skipped';

function rubricIsApproved(
  question: ExamWorkspaceState['questions'][number]
): boolean {
  const rubric = question.rubric;
  if (rubric?.status !== 'approved') {
    return false;
  }

  const basis = rubric.approvalBasis;
  if (!basis) {
    return true;
  }
  const approvedQuestionTextMatches =
    basis.questionText === question.text ||
    (questionAnalysisIsReady(question.analysis) &&
      question.analysis?.questionTextClean != null &&
      basis.questionText === question.analysis.questionTextClean);

  return (
    approvedQuestionTextMatches &&
    basis.questionContext === (question.analysis?.questionContext ?? '') &&
    basis.maxPoints === question.maxPoints &&
    criteriaMatchApprovalBasis(rubric.criteria, basis.criteria)
  );
}

function questionAnalysisIsReady(
  analysis: ExamWorkspaceState['questions'][number]['analysis'] | null | undefined
): boolean {
  return analysis?.status === 'ok';
}

function criteriaMatchApprovalBasis(
  current: NonNullable<ExamWorkspaceState['questions'][number]['rubric']>['criteria'],
  basis: NonNullable<
    NonNullable<ExamWorkspaceState['questions'][number]['rubric']>['approvalBasis']
  >['criteria']
): boolean {
  if (current.length !== basis.length) {
    return false;
  }
  return current.every((criterion, index) => {
    const approved = basis[index];
    return (
      approved?.criterionId === criterion.criterionId &&
      approved.label === criterion.label &&
      approved.points === criterion.points &&
      approved.partialCreditGuidance === criterion.partialCreditGuidance &&
      approved.source === criterion.source
    );
  });
}

function redactionWasSkipped(workspace: ExamWorkspaceState): boolean {
  return workspace.warnings.some((warning) => warning.code === REDACTION_SKIPPED_WARNING_CODE);
}

export function studentIntakePrerequisitesMet(workspace: ExamWorkspaceState): boolean {
  const redactionRequired = workspace.projectConfig?.redactionRequired ?? true;
  const redactionSatisfied =
    !redactionRequired || workspace.redactionRegions.length > 0 || redactionWasSkipped(workspace);
  const questions = workspace.questions;
  const questionsReady =
    questions.length > 0 &&
    questions.every((question) =>
      questionAnalysisIsReady(question.analysis) && rubricIsApproved(question)
    );

  return redactionSatisfied && questionsReady;
}
