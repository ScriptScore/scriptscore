// SPDX-License-Identifier: AGPL-3.0-only
import { get, writable } from 'svelte/store';

export type BusyAction =
  | 'create'
  | 'open'
  | 'close'
  | 'smoke'
  | 'saveQuestions'
  | 'saveRegions'
  | 'approve'
  | 'skipRedaction'
  | 'replaceTemplate'
  | 'exportTemplate'
  | 'saveSetup'
  | 'saveRubric'
  | 'generateRubric'
  | 'reAnalyze'
  | 'studentIntake'
  | 'studentWorkflow'
  | 'studentWorkflowRecovery'
  | 'resultsAssignment'
  | 'resultsUpload'
  | null;
export type WorkflowStep =
  | 'templateSetup'
  | 'students'
  | 'moderation'
  | 'exportResults'
  | 'settings';
export type TemplateSetupSubstep = 'setup' | 'review';

export interface WorkspaceViewState {
  activeWorkflowStep: WorkflowStep;
  activeTemplateSetupSubstep: TemplateSetupSubstep;
  selectedPageNumber: number;
  selectedQuestionId: string | null;
}

const initialState: WorkspaceViewState = {
  activeWorkflowStep: 'templateSetup',
  activeTemplateSetupSubstep: 'setup',
  selectedPageNumber: 1,
  selectedQuestionId: null
};

function createWorkspaceViewStore() {
  const { subscribe, set, update } = writable<WorkspaceViewState>(initialState);

  return {
    subscribe,
    reset() {
      set(initialState);
    },
    setWorkflowStep(activeWorkflowStep: WorkflowStep) {
      update((current) => ({ ...current, activeWorkflowStep }));
    },
    setTemplateSetupSubstep(activeTemplateSetupSubstep: TemplateSetupSubstep) {
      update((current) => ({ ...current, activeTemplateSetupSubstep }));
    },
    setSelectedPageNumber(selectedPageNumber: number) {
      update((current) => ({ ...current, selectedPageNumber }));
    },
    setSelectedQuestionId(selectedQuestionId: string | null) {
      update((current) => ({ ...current, selectedQuestionId }));
    },
    syncQuestionSelection(questionIds: string[], preferredQuestionId: string | null) {
      update((current) => ({
        ...current,
        selectedQuestionId: questionIds.includes(preferredQuestionId ?? '')
          ? preferredQuestionId
          : questionIds[0] ?? null
      }));
    },
    syncPageSelection(pageNumbers: number[], preferredPageNumber: number) {
      update((current) => ({
        ...current,
        selectedPageNumber: pageNumbers.includes(preferredPageNumber)
          ? preferredPageNumber
          : pageNumbers[0] ?? 1
      }));
    }
  };
}

export const workspaceView = createWorkspaceViewStore();

/** True when the Template → Exam Review surface is shown (not the Setup substep). */
export function isExamReviewSurfaceVisible(): boolean {
  const v = get(workspaceView);
  return v.activeWorkflowStep === 'templateSetup' && v.activeTemplateSetupSubstep === 'review';
}
