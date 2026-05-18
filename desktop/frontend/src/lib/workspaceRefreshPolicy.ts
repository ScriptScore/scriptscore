// SPDX-License-Identifier: AGPL-3.0-only
import type { RuntimeJobEvent } from '$lib/types';

export function isWorkflowRuntimeCommand(commandName: string): boolean {
  return (
    commandName === 'begin_student_workflow' ||
    commandName === 'regrade_question_answers' ||
    commandName === 'confirm_student_alignment' ||
    commandName === 'confirm_student_detect_review' ||
    commandName === 'confirm_student_parse_review' ||
    commandName.startsWith('scans.') ||
    commandName.startsWith('grading.')
  );
}

export function shouldRefreshWorkspaceAfterTerminalJob(event: RuntimeJobEvent): boolean {
  return (
    event.commandName === 'exam.analyze' ||
    event.commandName === 'exam.generate-rubric' ||
    isWorkflowRuntimeCommand(event.commandName)
  );
}

export function shouldEnsureAutomaticRubricsAfterTerminalJob(event: RuntimeJobEvent): boolean {
  return event.commandName === 'exam.analyze' && event.eventType === 'job_finished';
}

export function shouldRefreshWorkspaceDuringRuntimeEvent(event: RuntimeJobEvent): boolean {
  return (
    event.payload.workflowStateUpdated === true ||
    ((event.eventType === 'job_submitted' || event.eventType === 'job_started') &&
      isWorkflowRuntimeCommand(event.commandName))
  );
}
