// SPDX-License-Identifier: AGPL-3.0-only
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it } from 'vitest';

import { isExamReviewSurfaceVisible, workspaceView } from './workspaceView';

describe('workspace view store', () => {
  beforeEach(() => {
    workspaceView.reset();
  });

  it('preserves preferred selections when they still exist', () => {
    workspaceView.setSelectedPageNumber(2);
    workspaceView.setSelectedQuestionId('question_2');
    workspaceView.syncPageSelection([1, 2, 3], 2);
    workspaceView.syncQuestionSelection(['question_1', 'question_2'], 'question_2');

    expect(get(workspaceView).selectedPageNumber).toBe(2);
    expect(get(workspaceView).selectedQuestionId).toBe('question_2');
  });

  it('reports Exam Review visibility only on template setup + review substep', () => {
    expect(isExamReviewSurfaceVisible()).toBe(false);
    workspaceView.setTemplateSetupSubstep('review');
    expect(isExamReviewSurfaceVisible()).toBe(true);
    workspaceView.setWorkflowStep('students');
    expect(isExamReviewSurfaceVisible()).toBe(false);
  });

  it('falls back to the first available page and question when preferred values disappear', () => {
    workspaceView.setSelectedPageNumber(4);
    workspaceView.setSelectedQuestionId('question_9');
    workspaceView.syncPageSelection([1, 3], 4);
    workspaceView.syncQuestionSelection(['question_1', 'question_2'], 'question_9');

    expect(get(workspaceView).selectedPageNumber).toBe(1);
    expect(get(workspaceView).selectedQuestionId).toBe('question_1');
  });
});
