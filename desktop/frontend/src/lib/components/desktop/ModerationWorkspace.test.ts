// SPDX-License-Identifier: AGPL-3.0-only
import { tick } from 'svelte';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ModerationWorkspace from './ModerationWorkspace.svelte';
import type { ExamWorkspaceState } from '$lib/types';

function moderationWorkspaceState(): ExamWorkspaceState {
  return {
    project: {
      projectId: 'proj_1',
      displayName: 'Midterm 1',
      subject: 'Biology',
      courseCode: 'BIO 101',
      lmsCourseId: null,
      projectPath: '/tmp/project',
      createdAt: '1',
      updatedAt: '1'
    },
    status: 'approved',
    statusLabel: 'Ready for moderation',
    failureMessage: null,
    templatePreviewArtifacts: [],
    questions: [
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Explain the process.',
        baselinePdfText: 'Explain the process.',
        sourceArtifactId: null
      }
    ],
    redactionRegions: [],
    warnings: [],
    canApprove: true,
    canApproveRubric: true,
    projectConfig: {
      projectId: 'proj_1',
      displayName: 'Midterm 1',
      subject: 'Biology',
      courseCode: 'BIO 101',
      lmsCourseId: null,
      redactionRequired: true,
      instructorProfile: {
        gradingStrictness: 'balanced',
        syntaxLeniency: 'medium',
        ocrTolerance: 'medium',
        partialCreditStyle: 'balanced',
        feedbackStyle: 'brief',
        additionalGuidance: '',
        includeMinimumCreditCriterion: false,
        minimumCreditPercent: 10
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
      status: 'graded',
      latestJobId: null,
      submissions: [
        {
          studentRef: 'student_1',
          canonicalPdfPath: '/tmp/student_1.pdf',
          pageCount: 1,
          stage: 'graded',
          latestJobId: null,
          failureMessage: null,
          warnings: [],
          pageArtifacts: [
            {
              pageNumber: 1,
              imagePath: '/tmp/page-1.png',
              sourcePdfPath: '/tmp/student_1.pdf'
            }
          ],
          alignmentPages: [],
          answers: [
            {
              questionId: 'question_1',
              questionNumber: 1,
              cropImagePath: '/tmp/crop-1.png',
              moderationEligible: true,
              manualGradingRequired: false,
              manualGradingReason: null,
              parseStatus: 'ok',
              parseConfidence: 'high',
              parseConfidenceSource: 'combined',
              rawParsedText: 'raw answer text',
              verifiedText: 'accurate but wrong ending',
              reviewRequired: false,
              verified: true,
              stale: false,
              gradingStatus: 'draft_ready',
              gradingConfidence: 'high',
              gradingConfidenceReason: null,
              questionMaxPoints: 5,
              totalPointsAwarded: 4,
              feedbackText: 'Strong overall response.',
              criterionResults: [
                {
                  criterionIndex: 0,
                  label: 'Accuracy',
                  points: 3,
                  pointsAwarded: 2,
                  rationale: 'Mostly correct.'
                }
              ],
              highlights: [
                {
                  kind: 'correct',
                  startChar: 0,
                  endChar: 8,
                  text: 'accurate'
                },
                {
                  kind: 'incorrect',
                  startChar: 13,
                  endChar: 26,
                  text: 'wrong ending'
                }
              ],
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
    },
    workflowStage: 'student_grading_complete',
    workflowLabel: 'Grading complete'
  };
}

function moderationWorkspaceStateWithTwoQuestions(): ExamWorkspaceState {
  const state = moderationWorkspaceState();
  state.questions = [
    ...state.questions,
    {
      questionId: 'question_2',
      questionNumber: 2,
      pageNumber: 2,
      maxPoints: 3,
      text: 'Define osmosis.',
      baselinePdfText: 'Define osmosis.',
      sourceArtifactId: null
    }
  ];
  state.studentWorkflow!.submissions[0]!.pageCount = 2;
  state.studentWorkflow!.submissions[0]!.pageArtifacts = [
    ...state.studentWorkflow!.submissions[0]!.pageArtifacts,
    {
      pageNumber: 2,
      imagePath: '/tmp/page-2.png',
      sourcePdfPath: '/tmp/student_1.pdf'
    }
  ];
  state.studentWorkflow!.submissions[0]!.answers = [
    ...state.studentWorkflow!.submissions[0]!.answers,
    {
      questionId: 'question_2',
      questionNumber: 2,
      cropImagePath: '/tmp/crop-2.png',
      moderationEligible: true,
      manualGradingRequired: false,
      manualGradingReason: null,
      parseStatus: 'ok',
      parseConfidence: 'high',
      parseConfidenceSource: 'combined',
      rawParsedText: 'osmosis raw answer',
      verifiedText: 'water moves across a membrane',
      reviewRequired: false,
      verified: true,
      stale: false,
      gradingStatus: 'draft_ready',
      gradingConfidence: 'high',
      gradingConfidenceReason: null,
      questionMaxPoints: 3,
      totalPointsAwarded: 2,
      feedbackText: 'Good definition.',
      criterionResults: [
        {
          criterionIndex: 0,
          label: 'Definition',
          points: 3,
          pointsAwarded: 2,
          rationale: 'Names the movement but misses concentration gradient.'
        }
      ],
      highlights: [],
      warnings: []
    }
  ];
  return state;
}

function createDataTransfer() {
  const values = new Map<string, string>();
  return {
    effectAllowed: 'move',
    dropEffect: 'move',
    setData(type: string, value: string) {
      values.set(type, value);
    },
    getData(type: string) {
      return values.get(type) ?? '';
    }
  };
}

describe('ModerationWorkspace', () => {
  it('renders compact moderation content with highlighted text and crop image', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    expect(await screen.findByRole('button', { name: 'Q1' })).toBeTruthy();
    expect(
      screen.getByText((_, element) => element?.textContent === 'Q1 - Explain the process.')
    ).toBeTruthy();
    expect(screen.getByText('student_1')).toBeTruthy();

    const correct = screen.getByText('accurate');
    const incorrect = screen.getByText('wrong ending');
    expect(correct.className).toContain('message-success-text');
    expect(incorrect.className).toContain('message-error-text');

    const image = screen.getByAltText('student_1 answer crop') as HTMLImageElement;
    expect(image.src).toContain('/tmp/crop-1.png');
    expect(screen.queryByRole('button', { name: 'Set score 2 for student_1' })).toBeNull();
  });

  it('hides empty lanes until the operator adds one', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    expect(screen.getByTestId('score-lane-4')).toBeTruthy();
    expect(screen.queryByTestId('score-lane-3')).toBeNull();
    expect(screen.getByRole('button', { name: 'Add lane' })).toBeTruthy();
    expect(screen.queryByRole('dialog', { name: 'Add point lane' })).toBeNull();
    const questionTitle = screen.getByText(
      (_, element) => element?.textContent === 'Q1 - Explain the process.'
    );
    expect(questionTitle.parentElement?.className).toContain('items-center');
    expect(questionTitle.parentElement?.className).not.toContain('flex-wrap');
  });

  it('supports adding a lane manually and dragging an answer card into it', async () => {
    const onSaveModeratedScore = vi.fn().mockResolvedValue(undefined);
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore,
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Add lane' }));
    expect(screen.getByRole('dialog', { name: 'Add point lane' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: '5 pt' }));
    await tick();

    const card = await screen.findByTestId('moderation-card-student_1-question_1');
    const lane = screen.getByTestId('score-lane-5');
    const dataTransfer = createDataTransfer();

    await fireEvent.dragStart(card, { dataTransfer });
    await fireEvent.dragOver(lane, { dataTransfer });
    await fireEvent.drop(lane, { dataTransfer });

    expect(onSaveModeratedScore).toHaveBeenCalledWith('student_1', 'question_1', 5);
  });

  it('renders a global size slider and size-driven card sizing', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));
    const slider = screen.getByRole('slider', { name: 'Answer card size' });
    const card = await screen.findByTestId('moderation-card-student_1-question_1');
    const cardGrid = card.parentElement;
    const feedback = screen.getByPlaceholderText('Feedback');
    const textEvidence = card.querySelector('div[style*="max-height"]');
    const imageEvidence = screen.getByAltText('student_1 answer crop');

    expect(slider).toBeTruthy();
    expect(slider.getAttribute('max')).toBe('192');
    expect(cardGrid?.className).toContain('justify-start');
    expect(cardGrid?.getAttribute('style')).toContain(
      'grid-template-columns: repeat(auto-fill, minmax(min(100%, 15.2rem), 15.2rem));'
    );
    expect(cardGrid?.getAttribute('style')).not.toContain('1fr');
    expect(card.getAttribute('style')).toContain('min-height: 11rem;');
    expect(card.getAttribute('style')).not.toMatch(/(^|;)\s*height:/);
    expect(textEvidence?.getAttribute('style')).toContain('max-height: 5.777777777777778rem;');
    expect(textEvidence?.getAttribute('style')).not.toMatch(/(^|;)\s*height:/);
    expect(imageEvidence.getAttribute('style')).toContain('max-height: 6.5rem;');
    expect(imageEvidence.getAttribute('style')).not.toMatch(/(^|;)\s*height:/);
    expect(feedback.getAttribute('style')).toContain('max-height: 4.75rem;');
    expect(feedback.getAttribute('style')).not.toMatch(/(^|;)\s*height:/);

    await fireEvent.input(slider, { currentTarget: { value: '192' }, target: { value: '192' } });

    expect(cardGrid?.getAttribute('style')).toContain(
      'grid-template-columns: repeat(auto-fill, minmax(min(100%, 31.2rem), 31.2rem));'
    );
    expect(cardGrid?.getAttribute('style')).not.toContain('1fr');
    expect(card.getAttribute('style')).toContain('min-height: 21rem;');
    expect(card.getAttribute('style')).not.toMatch(/(^|;)\s*height:/);
    expect(textEvidence?.getAttribute('style')).toContain('max-height: 7.5rem;');
    expect(imageEvidence.getAttribute('style')).toContain('max-height: 8rem;');
    expect(feedback.getAttribute('style')).toContain('max-height: 4.75rem;');
    expect(feedback.getAttribute('style')).toContain('font-size: 1.13rem;');
  });

  it('shows evidence controls inside the settings popover', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    expect(screen.queryByRole('button', { name: 'PNG' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));

    expect(screen.getByRole('dialog', { name: 'Moderation view settings' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'PNG' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Text' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Both' })).toBeTruthy();
  });

  it('opens the moderation tips guide from the question header', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation tips' }));

    expect(screen.getByRole('dialog', { name: 'Moderation tips' })).toBeTruthy();
    const guideImage = screen.getByRole('img', {
      name: 'Synthetic guide showing moderation question tabs, score lanes, outlier review, and accept action'
    }) as HTMLImageElement;
    expect(guideImage.getAttribute('src')).toBe('/moderation-question-selection-tips.png');
    expect(screen.getByText(/vertical score lanes/)).toBeTruthy();
    expect(screen.getByText(/Accept in the question tab/)).toBeTruthy();
  });

  it('closes the settings popover on outside pointer and Escape and returns focus', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    const trigger = screen.getByRole('button', { name: 'Moderation view settings' });
    await fireEvent.click(trigger);
    expect(screen.getByRole('dialog', { name: 'Moderation view settings' })).toBeTruthy();

    await fireEvent.pointerDown(document.body);
    await tick();

    expect(screen.queryByRole('dialog', { name: 'Moderation view settings' })).toBeNull();
    expect(document.activeElement).toBe(trigger);

    await fireEvent.click(trigger);
    expect(screen.getByRole('dialog', { name: 'Moderation view settings' })).toBeTruthy();

    await fireEvent.keyDown(document, { key: 'Escape' });
    await tick();

    expect(screen.queryByRole('dialog', { name: 'Moderation view settings' })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('changes rendered evidence content from the shared view selector', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Text' }));
    await tick();

    expect(screen.getByText('accurate')).toBeTruthy();
    expect(screen.queryByAltText('student_1 answer crop')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'PNG' }));
    await tick();

    expect(screen.queryByText('accurate')).toBeNull();
    expect(screen.getByAltText('student_1 answer crop')).toBeTruthy();
  });

  it('keeps cards anonymous by default and reveals live names when toggled', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      studentDisplayNamesByRef: { student_1: 'Ada Lovelace' },
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    expect(screen.getByText('student_1')).toBeTruthy();
    expect(screen.queryByText('Ada Lovelace')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));
    const nameToggle = screen.getByRole('button', { name: /Actual student names/i });
    await fireEvent.click(nameToggle);
    await tick();

    expect(nameToggle.getAttribute('aria-pressed')).toBe('true');
    expect(
      screen.getAllByText((_, element) => element?.textContent?.includes('Ada Lovelace') ?? false)
        .length
    ).toBeGreaterThan(0);
    expect(screen.queryByText('student_1')).toBeNull();
  });

  it('uses active underline styling for questions and semantic selected styling for evidence controls', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    const questionButton = await screen.findByRole('button', { name: 'Q1' });
    const questionTab = questionButton.closest('[role="group"]');
    expect(questionTab?.className).toContain('text-text-primary');
    expect(questionTab?.querySelector('.bg-primary')).not.toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));

    const bothButton = screen.getByRole('button', { name: 'Both' });
    expect(bothButton.className).toContain('bg-interaction-selected');
    expect(bothButton.className).toContain('text-interaction-selected-text');
  });

  it('persists edited feedback on blur', async () => {
    const onSaveModeratedFeedback = vi.fn().mockResolvedValue(undefined);
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback,
      onSetQuestionReviewed: vi.fn()
    });

    const feedback = screen.getByPlaceholderText('Feedback');
    await fireEvent.input(feedback, {
      currentTarget: { value: 'Edited by moderator' },
      target: { value: 'Edited by moderator' }
    });
    await fireEvent.blur(feedback);

    expect(onSaveModeratedFeedback).toHaveBeenCalledWith(
      'student_1',
      'question_1',
      'Edited by moderator'
    );
  });

  it('marks score-edited student names and exposes the prior total as a tooltip', async () => {
    const state = moderationWorkspaceState();
    state.moderationState = {
      scoreOverrides: [
        {
          studentRef: 'student_1',
          questionId: 'question_1',
          moderatedTotalPoints: 5
        }
      ],
      feedbackOverrides: [],
      questionReviews: []
    };

    render(ModerationWorkspace, {
      workspaceState: state,
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    const editedName = document.querySelector('[title="Edited * was 4"]');
    expect(editedName?.textContent?.trim()).toBe('student_1*');
    expect(screen.queryByText('Edited · was 4')).toBeNull();
  });

  it('marks the selected question reviewed', async () => {
    const onSetQuestionReviewed = vi.fn().mockResolvedValue(true);
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));

    expect(onSetQuestionReviewed).toHaveBeenCalledWith('question_1', true);
  });

  it('shows Undo on the active reviewed question tab', async () => {
    const state = moderationWorkspaceState();
    state.moderationState = {
      scoreOverrides: [],
      feedbackOverrides: [],
      questionReviews: [{ questionId: 'question_1', reviewedAt: '2026-04-26T00:00:00Z' }]
    };

    render(ModerationWorkspace, {
      workspaceState: state,
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    expect(screen.getByRole('button', { name: 'Undo' })).toBeTruthy();
    expect(screen.queryByTestId('reviewed-tab-indicator-question_1')).toBeNull();
  });

  it('advances to the next unreviewed question after marking reviewed', async () => {
    const onSetQuestionReviewed = vi.fn().mockResolvedValue(true);
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceStateWithTwoQuestions(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await tick();

    expect(onSetQuestionReviewed).toHaveBeenCalledWith('question_1', true);
    expect(
      screen.getByText((_, element) => element?.textContent === 'Q2 - Define osmosis.')
    ).toBeTruthy();
  });

  it('does not advance when marking a question unreviewed', async () => {
    const state = moderationWorkspaceStateWithTwoQuestions();
    state.moderationState = {
      scoreOverrides: [],
      feedbackOverrides: [],
      questionReviews: [{ questionId: 'question_1', reviewedAt: '2026-04-26T00:00:00Z' }]
    };
    const onSetQuestionReviewed = vi.fn().mockResolvedValue(true);

    render(ModerationWorkspace, {
      workspaceState: state,
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await tick();

    expect(onSetQuestionReviewed).toHaveBeenCalledWith('question_1', false);
    expect(
      screen.getByText((_, element) => element?.textContent === 'Q1 - Explain the process.')
    ).toBeTruthy();
  });

  it('keeps the selected question when no later unreviewed question exists', async () => {
    const onSetQuestionReviewed = vi.fn().mockResolvedValue(true);
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await tick();

    expect(
      screen.getByText((_, element) => element?.textContent === 'Q1 - Explain the process.')
    ).toBeTruthy();
  });

  it('does not advance when marking reviewed fails to persist', async () => {
    const onSetQuestionReviewed = vi.fn().mockRejectedValue(new Error('save failed'));
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceStateWithTwoQuestions(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await tick();

    expect(onSetQuestionReviewed).toHaveBeenCalledWith('question_1', true);
    expect(
      screen.getByText((_, element) => element?.textContent === 'Q1 - Explain the process.')
    ).toBeTruthy();
  });

  it('does not advance when the review callback resolves with a failed persistence signal', async () => {
    const onSetQuestionReviewed = vi.fn().mockResolvedValue(false);
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceStateWithTwoQuestions(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await tick();

    expect(onSetQuestionReviewed).toHaveBeenCalledWith('question_1', true);
    expect(
      screen.getByText((_, element) => element?.textContent === 'Q1 - Explain the process.')
    ).toBeTruthy();
  });

  it('toggles an answer card into mini format without losing identity, status, or drag', async () => {
    const state = moderationWorkspaceState();
    state.studentWorkflow!.submissions[0]!.answers[0]!.reviewRequired = true;

    render(ModerationWorkspace, {
      workspaceState: state,
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    const card = await screen.findByTestId('moderation-card-student_1-question_1');
    expect(card.getAttribute('draggable')).toBe('true');
    expect(screen.getByText('student_1')).toBeTruthy();
    expect(screen.getByText('Review')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Use mini card for student_1' }));
    await tick();

    expect(screen.getByText('student_1')).toBeTruthy();
    expect(screen.getByText('Review')).toBeTruthy();
    expect(screen.queryByText('accurate')).toBeNull();
    expect(screen.queryByPlaceholderText('Feedback')).toBeNull();
    expect(screen.getByRole('button', { name: 'Use full card for student_1' })).toBeTruthy();
    const compactClasses = card.className.split(/\s+/);
    expect(compactClasses).toContain('justify-self-start');
    expect(compactClasses).toContain('self-start');
    expect(compactClasses).toContain('w-fit');
    expect(compactClasses).not.toContain('w-full');
    expect(card.getAttribute('style')).toContain('min-height: 0');
    expect(card.getAttribute('draggable')).toBe('true');
  });

  it('moves mini cards to the end of their score lane', async () => {
    const state = moderationWorkspaceState();
    const secondSubmission = structuredClone(state.studentWorkflow!.submissions[0]!);
    secondSubmission.studentRef = 'student_2';
    secondSubmission.canonicalPdfPath = '/tmp/student_2.pdf';
    secondSubmission.pageArtifacts = [
      {
        pageNumber: 1,
        imagePath: '/tmp/page-2.png',
        sourcePdfPath: '/tmp/student_2.pdf'
      }
    ];
    secondSubmission.answers = secondSubmission.answers.map((answer) => ({
      ...answer,
      cropImagePath: '/tmp/crop-2.png',
      verifiedText: 'second answer text'
    }));
    state.studentWorkflow!.submissions = [
      state.studentWorkflow!.submissions[0]!,
      secondSubmission
    ];

    render(ModerationWorkspace, {
      workspaceState: state,
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Use mini card for student_1' }));
    await tick();

    const cards = screen
      .getByTestId('score-lane-4')
      .querySelectorAll('[data-testid^="moderation-card-"]');
    expect(cards[0]?.getAttribute('data-testid')).toBe('moderation-card-student_2-question_1');
    expect(cards[1]?.getAttribute('data-testid')).toBe('moderation-card-student_1-question_1');
  });

  it('opens the full page preview with the matched page artifact image', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Preview full page for student_1' }));

    const dialog = await screen.findByRole('dialog', { name: 'student_1 page 1' });
    expect(dialog.getAttribute('style')).toContain('left:');
    expect(dialog.getAttribute('style')).toContain('top:');
    expect(dialog.className).toContain('bg-surface-overlay');
    expect(dialog.className).toContain('border-border-default');
    expect(dialog.hasAttribute('aria-modal')).toBe(false);
    const image = await screen.findByAltText('student_1 full page 1');
    expect((image as HTMLImageElement).src).toContain('/tmp/page-1.png');
    expect(image.parentElement?.className).toContain('w-fit');
  });

  it('opens the graded rubric preview with criterion points and rationale', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    const rubricButton = screen.getByRole('button', { name: 'Preview rubric for student_1' });
    expect(rubricButton.getAttribute('title')).toBe('Preview rubric for student_1');

    await fireEvent.click(rubricButton);

    expect(await screen.findByRole('dialog', { name: 'student_1 graded rubric' })).toBeTruthy();
    expect(screen.getByRole('dialog', { name: 'student_1 graded rubric' }).className).toContain(
      'bg-surface-overlay'
    );
    expect(screen.getByText('Accuracy')).toBeTruthy();
    expect(screen.getByText('2/3')).toBeTruthy();
    expect(screen.getByText('Mostly correct.')).toBeTruthy();
    expect(screen.getByText((_, element) => element?.textContent === 'Total: 4 / 5')).toBeTruthy();
  });

  it('closes previews on outside pointer and Escape and returns focus to the trigger', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    const pageTrigger = screen.getByRole('button', { name: 'Preview full page for student_1' });
    await fireEvent.click(pageTrigger);
    expect(await screen.findByRole('dialog', { name: 'student_1 page 1' })).toBeTruthy();

    await fireEvent.pointerDown(document.body);
    await tick();

    expect(screen.queryByRole('dialog', { name: 'student_1 page 1' })).toBeNull();
    expect(document.activeElement).toBe(pageTrigger);

    const rubricTrigger = screen.getByRole('button', { name: 'Preview rubric for student_1' });
    await fireEvent.click(rubricTrigger);
    expect(await screen.findByRole('dialog', { name: 'student_1 graded rubric' })).toBeTruthy();

    await fireEvent.keyDown(window, { key: 'Escape' });
    await tick();

    expect(screen.queryByRole('dialog', { name: 'student_1 graded rubric' })).toBeNull();
    expect(document.activeElement).toBe(rubricTrigger);
  });

  it('does not reopen a preview when its trigger receives the outside click that closes it', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    const pageTrigger = screen.getByRole('button', { name: 'Preview full page for student_1' });
    await fireEvent.click(pageTrigger);
    expect(await screen.findByRole('dialog', { name: 'student_1 page 1' })).toBeTruthy();

    await fireEvent.pointerDown(pageTrigger);
    await fireEvent.click(pageTrigger);
    await tick();

    expect(screen.queryByRole('dialog', { name: 'student_1 page 1' })).toBeNull();
    expect(document.activeElement).toBe(pageTrigger);
  });

  it('keeps preview actions anonymous until live names are explicitly shown', async () => {
    render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      studentDisplayNamesByRef: { student_1: 'Ada Lovelace' },
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    expect(screen.getByRole('button', { name: 'Preview full page for student_1' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Preview full page for Ada Lovelace' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Actual student names/i }));
    await tick();

    expect(screen.getByRole('button', { name: 'Preview full page for Ada Lovelace' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Preview full page for student_1' })).toBeNull();
  });

  it('refreshes live names when the roster cache becomes ready after names are enabled', async () => {
    const view = render(ModerationWorkspace, {
      workspaceState: moderationWorkspaceState(),
      studentDisplayNamesByRef: {},
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Actual student names/i }));
    await tick();

    expect(screen.getByText('student_1')).toBeTruthy();
    expect(screen.queryByText('Ada Lovelace')).toBeNull();

    await view.rerender({
      workspaceState: moderationWorkspaceState(),
      studentDisplayNamesByRef: { student_1: 'Ada Lovelace' },
      busy: false,
      onSaveModeratedScore: vi.fn(),
      onSaveModeratedFeedback: vi.fn(),
      onSetQuestionReviewed: vi.fn()
    });

    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.queryByText('student_1')).toBeNull();
  });
});
