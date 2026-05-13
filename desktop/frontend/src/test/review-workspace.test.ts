// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, within, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ensureAutomaticRubricJobs } from '$lib/desktop';
import ReviewWorkspace from '$lib/components/desktop/ReviewWorkspace.svelte';
import { onRuntimeJobEvent } from '$lib/stores/shell';
import type { ExamWorkspaceState, QuestionEdit, RubricCriterion, RuntimeJobEvent } from '$lib/types';

vi.mock('$lib/desktop', () => ({
  getJobTrace: vi.fn(),
  ensureAutomaticRubricJobs: vi.fn().mockResolvedValue(undefined),
  isDesktopHost: vi.fn(() => true),
  toDesktopAssetUrl: (path: string) => `asset://${path}`
}));

vi.mock('$lib/stores/appSettings', () => ({
  appSettings: {
    subscribe: (fn: (v: unknown) => void) => {
      fn({
        aiAssistEnabled: true,
        llmProvider: 'ollama',
        llmBaseUrl: 'http://127.0.0.1:11434',
        llmModel: 'llama',
        llmApiKey: null,
        lmsProvider: 'none',
        lmsCanvasBaseUrl: '',
        lmsCanvasApiKey: null,
        lmsBindingSecretPlaintextFallback: false,
        piiPaddleModelDir: null,
        projectsDirectory: null,
        instructorProfile: {},
        theme: 'system',
        aiAssistCategories: {
          rubrics: true,
          questionAnalysis: false,
          gradingFeedback: false,
          parsingReview: false
        },
        onboardingCompleted: true
      });
      return () => {};
    }
  }
}));

vi.mock('$lib/stores/shell', () => ({
  onRuntimeJobEvent: vi.fn(() => () => {})
}));

function runtimeJobEvent(overrides: Partial<RuntimeJobEvent> = {}): RuntimeJobEvent {
  return {
    eventType: 'job_queued',
    commandName: 'exam.generate-rubric',
    workerStatus: 'busy',
    requestId: 'request_1',
    jobId: 'job_rubric_runtime_1',
    payload: { questionId: 'question_1' },
    ...overrides
  };
}

function criterion(points: number): RubricCriterion {
  return {
    criterionId: `criterion_${points}`,
    label: 'Correctness',
    points,
    partialCreditGuidance: 'Award credit.',
    source: 'manual'
  };
}

function rubricStatusText(): string | null | undefined {
  return screen.getByText('Rubric status:').querySelector('span')?.textContent;
}

function buttonDisabled(name: string): boolean {
  return (screen.getByRole('button', { name }) as HTMLButtonElement).disabled;
}

function buildWorkspaceState(): ExamWorkspaceState {
  return {
    project: {
      projectId: 'proj_1',
      displayName: 'Midterm 1',
      subject: 'Chemistry',
      courseCode: 'CHEM 201',
      lmsCourseId: null,
      projectPath: '/tmp/project',
      createdAt: '1',
      updatedAt: '1'
    },
    status: 'approved',
    statusLabel: 'Ready for review',
    failureMessage: null,
    templatePreviewArtifacts: [],
    redactionRegions: [],
    warnings: [],
    canApprove: true,
    canApproveRubric: true,
    workflowStage: 'template_setup',
    workflowLabel: 'Template setup',
    projectConfig: {
      projectId: 'proj_1',
      displayName: 'Midterm 1',
      subject: 'Chemistry',
      courseCode: 'CHEM 201',
      lmsCourseId: null,
      redactionRequired: true,
      instructorProfile: {
        gradingStrictness: '',
        syntaxLeniency: '',
        ocrTolerance: '',
        partialCreditStyle: '',
        feedbackStyle: '',
        additionalGuidance: '',
        includeMinimumCreditCriterion: false,
        minimumCreditPercent: 0
      },
      traceRefs: {
        setupJobId: null,
        batchAnalyzeJobId: null,
        batchRubricJobId: null,
        intakeJobId: null
      },
      createdAt: '1',
      updatedAt: '1'
    },
    studentRoster: [],
    studentIntake: {
      status: 'ready',
      latestJobId: null,
      items: [],
      unresolvedCount: 0
    },
    studentWorkflow: {
      status: 'idle',
      latestJobId: null,
      submissions: []
    },
    questions: [
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Question one',
        baselinePdfText: 'Question one',
        sourceArtifactId: null,
        imagePath: null,
        analysis: {
          status: 'ok',
          questionTextClean: 'Question one',
          questionContext: '',
          warnings: [],
          latestJobId: 'job_analyze_1'
        },
        rubric: {
          status: 'draft',
          criteria: [],
          warnings: [],
          approvedAt: null,
          latestJobId: 'job_rubric_1'
        }
      },
      {
        questionId: 'question_2',
        questionNumber: 2,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Question two',
        baselinePdfText: 'Question two',
        sourceArtifactId: null,
        imagePath: null,
        analysis: {
          status: 'ok',
          questionTextClean: 'Question two',
          questionContext: '',
          warnings: [],
          latestJobId: 'job_analyze_2'
        },
        rubric: {
          status: 'approved',
          criteria: [criterion(5)],
          warnings: [],
          approvedAt: '2026-04-08T00:00:00Z',
          latestJobId: 'job_rubric_2'
        }
      },
      {
        questionId: 'question_3',
        questionNumber: 3,
        pageNumber: 2,
        maxPoints: 5,
        text: 'Question three',
        baselinePdfText: 'Question three',
        sourceArtifactId: null,
        imagePath: null,
        analysis: {
          status: 'ok',
          questionTextClean: 'Question three',
          questionContext: '',
          warnings: [],
          latestJobId: 'job_analyze_3'
        },
        rubric: {
          status: 'draft',
          criteria: [],
          warnings: [],
          approvedAt: null,
          latestJobId: 'job_rubric_3'
        }
      }
    ]
  };
}

function buildQuestionDrafts(): QuestionEdit[] {
  return [
    {
      questionId: 'question_1',
      questionNumber: 1,
      pageNumber: 1,
      maxPoints: 5,
      text: 'Question one',
      questionContext: ''
    },
    {
      questionId: 'question_2',
      questionNumber: 2,
      pageNumber: 1,
      maxPoints: 5,
      text: 'Question two',
      questionContext: ''
    },
    {
      questionId: 'question_3',
      questionNumber: 3,
      pageNumber: 2,
      maxPoints: 5,
      text: 'Question three',
      questionContext: ''
    }
  ];
}

describe('ReviewWorkspace', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(ensureAutomaticRubricJobs).mockResolvedValue(undefined);
    vi.mocked(onRuntimeJobEvent).mockImplementation(() => () => {});
  });

  it('schedules ensureAutomaticRubricJobs after mount (deferred until after paint)', async () => {
    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });
    expect(ensureAutomaticRubricJobs).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(ensureAutomaticRubricJobs).toHaveBeenCalledTimes(1);
    });
  });

  it('shows one unified Save action and no setup approval action', () => {
    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    expect(screen.getAllByRole('button', { name: 'Save' })).toHaveLength(1);
    expect(screen.queryByRole('button', { name: 'Save edits' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Save rubric' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Approve setup' })).toBeNull();
  });

  it('enables unified Save for question edits, rubric edits, and combined edits', async () => {
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);

    const { unmount } = render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1',
      hasUnsavedQuestionEdits: true,
      onSaveReviewChanges
    });
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(false);
    unmount();

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1',
      onSaveReviewChanges
    });
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: '+ add criterion' }));
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(onSaveReviewChanges).toHaveBeenLastCalledWith(
      'question_1',
      expect.arrayContaining([expect.objectContaining({ points: 1 })]),
      false,
      true
    );
  });

  it('passes question and rubric dirty state to unified Save for combined edits', async () => {
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1',
      hasUnsavedQuestionEdits: true,
      onSaveReviewChanges
    });

    await fireEvent.click(screen.getByRole('button', { name: '+ add criterion' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onSaveReviewChanges).toHaveBeenCalledWith(
      'question_1',
      expect.arrayContaining([expect.objectContaining({ points: 1 })]),
      true,
      true
    );
  });

  it('uses only Save for approved rubric edits', () => {
    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_2'
    });

    expect(screen.getAllByRole('button', { name: 'Save' })).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Rescind approval' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Re Analyze' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Generate rubric' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Update approved rubric' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Approve rubric' })).toBeNull();
  });

  it('rescinds approved rubric status without requiring a content edit', async () => {
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_2',
      onSaveReviewChanges
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Rescind approval' }));

    expect(onSaveReviewChanges).toHaveBeenCalledWith(
      'question_2',
      expect.arrayContaining([expect.objectContaining({ points: 5 })]),
      false,
      true,
      'grading'
    );
  });

  it('keeps the direct approval rescind shortcut disabled while approved edits are dirty', async () => {
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_2',
      onSaveReviewChanges
    });

    await fireEvent.click(screen.getByRole('button', { name: '+ add criterion' }));

    expect((screen.getByRole('button', { name: 'Rescind approval' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('opens a three-choice decision before saving approved rubric edits', async () => {
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);
    const drafts = buildQuestionDrafts();
    drafts[1] = { ...drafts[1]!, text: 'Question two revised' };

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: drafts,
      selectedQuestionId: 'question_2',
      hasUnsavedQuestionEdits: true,
      onSaveReviewChanges
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(screen.getByRole('dialog', { name: 'Save approved rubric changes?' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Discard changes' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save as minor edit' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save and rescind approval' })).toBeTruthy();
    expect(onSaveReviewChanges).not.toHaveBeenCalled();
  });

  it('discards approved rubric edits from the decision dialog', async () => {
    const onDiscardReviewChanges = vi.fn();
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);
    const drafts = buildQuestionDrafts();
    drafts[1] = { ...drafts[1]!, text: 'Question two revised' };

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: drafts,
      selectedQuestionId: 'question_2',
      hasUnsavedQuestionEdits: true,
      onDiscardReviewChanges,
      onSaveReviewChanges
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Discard changes' }));

    expect(onDiscardReviewChanges).toHaveBeenCalledWith('question_2');
    expect(onSaveReviewChanges).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog', { name: 'Save approved rubric changes?' })).toBeNull();
  });

  it('passes minor impact when saving approved non-structural edits', async () => {
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);
    const drafts = buildQuestionDrafts();
    drafts[1] = { ...drafts[1]!, text: 'Question two revised' };

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: drafts,
      selectedQuestionId: 'question_2',
      hasUnsavedQuestionEdits: true,
      onSaveReviewChanges
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save as minor edit' }));

    expect(onSaveReviewChanges).toHaveBeenCalledWith(
      'question_2',
      expect.arrayContaining([expect.objectContaining({ points: 5 })]),
      true,
      false,
      'minor'
    );
  });

  it('passes grading impact when saving and rescinding approved rubric status', async () => {
    const onSaveReviewChanges = vi.fn().mockResolvedValue(undefined);
    const drafts = buildQuestionDrafts();
    drafts[1] = { ...drafts[1]!, text: 'Question two revised' };

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: drafts,
      selectedQuestionId: 'question_2',
      hasUnsavedQuestionEdits: true,
      onSaveReviewChanges
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save and rescind approval' }));

    expect(onSaveReviewChanges).toHaveBeenCalledWith(
      'question_2',
      expect.arrayContaining([expect.objectContaining({ points: 5 })]),
      true,
      false,
      'grading'
    );
  });

  it('disables minor save when approved rubric structure changes', async () => {
    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_2',
      onSaveReviewChanges: vi.fn().mockResolvedValue(undefined)
    });

    await fireEvent.click(screen.getByRole('button', { name: '+ add criterion' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect((screen.getByRole('button', { name: 'Save as minor edit' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText('Criteria were added, removed, or had point values changed. Minor save is unavailable for this edit.')).toBeTruthy();
  });

  it('does not show queued automatic-rubric badges until a runtime job is observed', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.status = 'approved';
    workspaceState.questions[0]!.imagePath = '/tmp/q1.png';
    workspaceState.questions[2]!.imagePath = '/tmp/q3.png';

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    const questionThreeButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 3/i
    });

    expect(within(questionOneButton).queryByLabelText('Rubric generation queued')).toBeNull();
    expect(within(questionThreeButton).queryByLabelText('Rubric generation queued')).toBeNull();
  });

  it('does not upscale the question PNG preview beyond its natural size', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.imagePath = '/tmp/q1.png';

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    await fireEvent.click(screen.getByTitle('Toggle question image preview'));

    const preview = screen.getByRole('img', { name: 'Question 1 preview' });
    expect(preview.getAttribute('src')).toBe('asset:///tmp/q1.png');
    expect(preview.classList.contains('max-w-full')).toBe(true);
    expect(preview.classList.contains('h-auto')).toBe(true);
    expect(preview.classList.contains('w-full')).toBe(false);
  });

  it('shows rubric queued and running icons only from runtime job events', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    vi.mocked(onRuntimeJobEvent).mockImplementationOnce((handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return () => {};
    });

    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    expect(within(questionOneButton).queryByLabelText('Rubric generation queued')).toBeNull();
    expect(rubricStatusText()).toBe('draft');
    expect(buttonDisabled('Generate rubric')).toBe(false);
    expect(buttonDisabled('+ add criterion')).toBe(false);

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(runtimeJobEvent());
    await waitFor(() => {
      expect(within(questionOneButton).getByLabelText('Rubric generation queued')).toBeTruthy();
      expect(rubricStatusText()).toBe('queued');
      expect(buttonDisabled('Generate rubric')).toBe(true);
      expect(buttonDisabled('+ add criterion')).toBe(true);
    });

    emitRuntimeEvent(runtimeJobEvent({ eventType: 'job_started' }));
    await waitFor(() => {
      expect(within(questionOneButton).getByLabelText('Generating rubric')).toBeTruthy();
      expect(rubricStatusText()).toBe('running');
      expect(buttonDisabled('Generate rubric')).toBe(true);
      expect(buttonDisabled('+ add criterion')).toBe(true);
    });

    emitRuntimeEvent(runtimeJobEvent({ eventType: 'job_finished', workerStatus: 'ready' }));
    await waitFor(() => {
      expect(within(questionOneButton).queryByLabelText('Generating rubric')).toBeNull();
      expect(within(questionOneButton).queryByLabelText('Rubric generation queued')).toBeNull();
      expect(rubricStatusText()).toBe('draft');
      expect(buttonDisabled('Generate rubric')).toBe(false);
      expect(buttonDisabled('+ add criterion')).toBe(false);
    });
  });

  it('shows an approved-rubric checkmark in the question sidebar', () => {
    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    const questionTwoButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 2/i
    });

    expect(within(questionOneButton).queryByLabelText('Rubric approved')).toBeNull();
    expect(within(questionTwoButton).getByLabelText('Rubric approved')).toBeTruthy();
  });

  it('shows analysis progress in the question sidebar and selected question panel', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.analysis = {
      status: 'running',
      questionTextClean: null,
      questionContext: null,
      warnings: [],
      latestJobId: 'job_analyze_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });

    expect(within(questionOneButton).getByLabelText('Analyzing question')).toBeTruthy();
    expect(screen.getByText('Analyzing question...')).toBeTruthy();
    expect(
      screen.queryByText(
        'Analysis has not completed yet. Question text edits stay disabled until cleaned text is ready.'
      )
    ).toBeNull();
    expect((screen.getByLabelText('Question text') as HTMLTextAreaElement).disabled).toBe(true);
  });

  it('shows analysis progress from an active batch analyze job before question status persists', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.analysis = {
      status: 'not_started',
      questionTextClean: null,
      questionContext: null,
      warnings: [],
      latestJobId: null
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1',
      analysisInProgress: true
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });

    expect(within(questionOneButton).getByLabelText('Analyzing question')).toBeTruthy();
    expect(screen.getByText('Analyzing question...')).toBeTruthy();
  });

  it('shows re-analysis progress for a question whose persisted analysis is already complete', () => {
    const workspaceState = buildWorkspaceState();

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1',
      analysisJobByQuestion: { question_1: 'running' }
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });

    expect(within(questionOneButton).getByLabelText('Analyzing question')).toBeTruthy();
    expect(screen.queryByText('Analyzing question...')).toBeNull();
    expect(within(questionOneButton).queryByLabelText('Rubric warning')).toBeNull();
  });

  it('switches the selected criterion detail from the nested criteria rail', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [
        {
          criterionId: 'c1',
          label: 'Correct setup',
          points: 3,
          partialCreditGuidance: 'Award setup credit.',
          source: 'manual'
        },
        {
          criterionId: 'c2',
          label: 'Evidence and explanation',
          points: 2,
          partialCreditGuidance: 'Award explanation credit.',
          source: 'manual'
        }
      ],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    expect((screen.getByLabelText('Label') as HTMLInputElement).value).toBe('Correct setup');

    const criterionList = screen.getByRole('navigation', { name: 'Criterion list' });
    await fireEvent.click(within(criterionList).getByRole('button', { name: /Evidence and explanation/ }));

    expect((screen.getByLabelText('Label') as HTMLInputElement).value).toBe('Evidence and explanation');
    expect(screen.queryByRole('button', { name: '0 points' })).toBeNull();
    expect(screen.getByRole('button', { name: '2 points' }).getAttribute('aria-pressed')).toBe('true');
    const criterionPointsLabel = screen
      .getAllByText('Points')
      .find((element) => element.className.includes('shell-meta'));
    expect(criterionPointsLabel).toBeTruthy();
    expect(
      screen.getByLabelText('Partial Credit Guidance').compareDocumentPosition(criterionPointsLabel!) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
  });

  it('selects the new criterion after adding one', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [criterion(5)],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    expect((screen.getByLabelText('Label') as HTMLInputElement).value).toBe('Correctness');

    await fireEvent.click(screen.getByRole('button', { name: '+ add criterion' }));

    expect((screen.getByLabelText('Label') as HTMLInputElement).value).toBe('');
    expect(screen.getAllByText('Criterion 2').length).toBeGreaterThan(0);
  });

  it('selects the next or previous criterion after removal', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [
        {
          criterionId: 'c1',
          label: 'First criterion',
          points: 1,
          partialCreditGuidance: '',
          source: 'manual'
        },
        {
          criterionId: 'c2',
          label: 'Second criterion',
          points: 2,
          partialCreditGuidance: '',
          source: 'manual'
        },
        {
          criterionId: 'c3',
          label: 'Third criterion',
          points: 2,
          partialCreditGuidance: '',
          source: 'manual'
        }
      ],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const criterionList = screen.getByRole('navigation', { name: 'Criterion list' });
    await fireEvent.click(within(criterionList).getByRole('button', { name: /Second criterion/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove criterion' }));
    expect(screen.getByRole('dialog', { name: 'Delete criterion?' })).toBeTruthy();
    expect(screen.getByText('This removes "Second criterion" from the rubric draft.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete criterion' }));

    expect((screen.getByLabelText('Label') as HTMLInputElement).value).toBe('Third criterion');

    await fireEvent.click(screen.getByRole('button', { name: 'Remove criterion' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Delete criterion' }));

    expect((screen.getByLabelText('Label') as HTMLInputElement).value).toBe('First criterion');
  });

  it('shows criterion rail points, warnings, minimum-credit status, and two-line label clamp', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [
        {
          criterionId: 'c1',
          label: 'A very long correctness criterion label that should fit inside two rail lines',
          points: 3,
          partialCreditGuidance: '',
          source: 'manual'
        },
        {
          criterionId: 'c2',
          label: 'Minimum effort',
          points: 2,
          partialCreditGuidance: '',
          source: 'minimum_credit'
        }
      ],
      warnings: [
        {
          code: 'criterion_scope_warning',
          message: 'Clarify this criterion.',
          scope: '{"criteria":[2]}'
        }
      ],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const criterionList = screen.getByRole('navigation', { name: 'Criterion list' });
    expect(within(criterionList).getByText('3 pts')).toBeTruthy();
    expect(within(criterionList).getByText('2 pts')).toBeTruthy();
    expect(within(criterionList).getByLabelText('Criterion warning')).toBeTruthy();
    expect(within(criterionList).getByLabelText('Minimum credit criterion')).toBeTruthy();
    expect(
      within(criterionList).getByText(
        'A very long correctness criterion label that should fit inside two rail lines'
      ).className
    ).toContain('line-clamp-2');

    await fireEvent.click(within(criterionList).getByRole('button', { name: /Minimum effort/ }));

    expect(screen.getByText('Clarify this criterion.')).toBeTruthy();
    expect(screen.queryByText('Minimum credit criterion')).toBeNull();
  });

  it('opens visible help popovers from rubric and question context info icons', async () => {
    render(ReviewWorkspace, {
      workspaceState: buildWorkspaceState(),
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Question context guidance' }));
    expect(screen.getByRole('dialog', { name: 'Question context guidance' })).toBeTruthy();
    expect(screen.getByText(/Natural-language context visible in the image/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Rubric criteria help' }));
    expect(screen.getByRole('dialog', { name: 'Rubric criteria help' })).toBeTruthy();
    expect(screen.getByText(/Edits stay in memory until you save/)).toBeTruthy();
  });

  it('shows queued analysis ahead of rubric warning and approved states in the question sidebar', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[1]!.analysis = {
      status: 'queued',
      questionTextClean: null,
      questionContext: null,
      warnings: [],
      latestJobId: 'job_analyze_2'
    };
    workspaceState.questions[1]!.rubric = {
      status: 'approved',
      criteria: [],
      warnings: [{ code: 'rubric_points_mismatch', message: 'Criteria point mismatch.', scope: null }],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_2'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionTwoButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 2/i
    });

    expect(within(questionTwoButton).getByLabelText('Question analysis queued')).toBeTruthy();
    expect(within(questionTwoButton).queryByLabelText('Rubric warning')).toBeNull();
    expect(within(questionTwoButton).queryByLabelText('Rubric approved')).toBeNull();
  });

  it('shows a rubric warning icon in the question sidebar', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[1]!.rubric = {
      status: 'approved',
      criteria: [],
      warnings: [
        {
          code: 'rubric_semantic_review_failed',
          message: 'Secondary rubric semantic review failed; local warnings were kept.',
          scope: null
        }
      ],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_2'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionTwoButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 2/i
    });

    expect(within(questionTwoButton).getByLabelText('Rubric warning')).toBeTruthy();
    expect(within(questionTwoButton).queryByLabelText('Rubric approved')).toBeNull();
  });

  it('updates the question sidebar warning while editing rubric points', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [
        {
          criterionId: 'c1',
          label: 'Correctness',
          points: 5,
          partialCreditGuidance: 'Award up to 5 points.',
          source: 'manual'
        }
      ],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    expect(within(questionOneButton).queryByLabelText('Rubric warning')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: '4 points' }));

    expect(within(questionOneButton).getByLabelText('Rubric warning')).toBeTruthy();
  });

  it('clears a stale point-mismatch sidebar warning when edited criteria match max points', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [
        {
          criterionId: 'c1',
          label: 'Correctness',
          points: 4,
          partialCreditGuidance: 'Award up to 4 points.',
          source: 'manual'
        }
      ],
      warnings: [{ code: 'rubric_points_mismatch', message: 'Criteria point mismatch.', scope: null }],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    expect(within(questionOneButton).getByLabelText('Rubric warning')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: '5 points' }));

    expect(within(questionOneButton).queryByLabelText('Rubric warning')).toBeNull();
  });

  it('shows a sidebar warning after removing the last rubric criterion', async () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [criterion(5)],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    expect(within(questionOneButton).queryByLabelText('Rubric warning')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Remove criterion' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Delete criterion' }));

    expect(within(questionOneButton).getByLabelText('Rubric warning')).toBeTruthy();
  });

  it('shows a saved rubric points warning in the question sidebar', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [
        {
          criterionId: 'c1',
          label: 'Correctness',
          points: 4,
          partialCreditGuidance: 'Award up to 4 points.',
          source: 'manual'
        }
      ],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });

    expect(within(questionOneButton).getByLabelText('Rubric warning')).toBeTruthy();
  });

  it('shows a saved empty rubric warning in the question sidebar', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });

    expect(within(questionOneButton).getByLabelText('Rubric warning')).toBeTruthy();
    expect(within(questionOneButton).queryByLabelText('Rubric generation queued')).toBeNull();
  });

  it('shows a rubric error icon in the question sidebar', () => {
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'error',
      criteria: [],
      warnings: [],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_1'
    };
    workspaceState.questions[1]!.rubric = {
      status: 'draft',
      criteria: [],
      warnings: [
        {
          code: 'rubric_generate_failed',
          message: 'Rubric generation failed.',
          scope: null
        }
      ],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_2'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    const questionTwoButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 2/i
    });

    expect(within(questionOneButton).getByLabelText('Rubric error')).toBeTruthy();
    expect(within(questionTwoButton).getByLabelText('Rubric error')).toBeTruthy();
    expect(within(questionTwoButton).queryByLabelText('Rubric warning')).toBeNull();
    expect(within(questionTwoButton).queryByLabelText('Rubric approved')).toBeNull();
  });

  it('prioritizes analysis, rubric generation, rubric errors, warnings, then approved state', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    vi.mocked(onRuntimeJobEvent).mockImplementationOnce((handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return () => {};
    });
    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.analysis = {
      status: 'running',
      questionTextClean: null,
      questionContext: null,
      warnings: [],
      latestJobId: 'job_analyze_1'
    };
    workspaceState.questions[0]!.rubric = {
      status: 'error',
      criteria: [],
      warnings: [{ code: 'rubric_generate_failed', message: 'Rubric generation failed.', scope: null }],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_1'
    };
    workspaceState.questions[1]!.imagePath = '/tmp/q2.png';
    workspaceState.questions[1]!.rubric = {
      status: 'error',
      criteria: [],
      warnings: [{ code: 'rubric_generate_failed', message: 'Rubric generation failed.', scope: null }],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_2'
    };
    workspaceState.questions[2]!.rubric = {
      status: 'approved',
      criteria: [],
      warnings: [{ code: 'rubric_points_mismatch', message: 'Criteria point mismatch.', scope: null }],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_3'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1'
    });

    const sidebar = document.querySelector('aside');
    expect(sidebar).toBeTruthy();
    const questionOneButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 1/i
    });
    const questionTwoButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 2/i
    });
    const questionThreeButton = within(sidebar as HTMLElement).getByRole('button', {
      name: /Question 3/i
    });

    expect(within(questionOneButton).getByLabelText('Analyzing question')).toBeTruthy();
    expect(within(questionOneButton).queryByLabelText('Rubric error')).toBeNull();
    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;
    emitRuntimeEvent(
      runtimeJobEvent({
        jobId: 'job_rubric_runtime_2',
        payload: { questionId: 'question_2' }
      })
    );
    await waitFor(() => {
      expect(within(questionTwoButton).getByLabelText('Rubric generation queued')).toBeTruthy();
    });
    expect(within(questionTwoButton).queryByLabelText('Rubric error')).toBeNull();
    expect(within(questionThreeButton).getByLabelText('Rubric warning')).toBeTruthy();
    expect(within(questionThreeButton).queryByLabelText('Rubric approved')).toBeNull();
  });

  it('advances to the next question without an approved rubric after approval', async () => {
    const onSaveRubric = vi.fn().mockResolvedValue(undefined);
    const onSelectQuestion = vi.fn();

    const workspaceState = buildWorkspaceState();
    workspaceState.questions[0]!.rubric = {
      status: 'draft',
      criteria: [{ criterionId: 'c1', label: 'Overall quality', points: 5, partialCreditGuidance: 'Award between 0 and 5.', source: 'generated' }],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1',
      onSaveRubric,
      onSelectQuestion
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Approve rubric' }));

    await waitFor(() => {
      expect(onSaveRubric).toHaveBeenCalledWith('question_1', expect.arrayContaining([expect.objectContaining({ criterionId: 'c1', points: 5 })]), true);
      expect(onSelectQuestion).toHaveBeenCalledWith('question_3');
    });
  });

  it('shows a points mismatch warning and blocks approval when criteria sum does not equal max points', async () => {
    const onSaveRubric = vi.fn().mockResolvedValue(undefined);

    const workspaceState = buildWorkspaceState();

    render(ReviewWorkspace, {
      workspaceState,
      questionDrafts: buildQuestionDrafts(),
      selectedQuestionId: 'question_1',
      onSaveRubric
    });

    expect(screen.getByText(/Criteria points sum to 0, but question max points is 5/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Approve rubric' }));

    expect(onSaveRubric).not.toHaveBeenCalled();
  });
});
