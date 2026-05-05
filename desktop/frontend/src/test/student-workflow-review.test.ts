// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { appSettings } from '$lib/stores/appSettings';
import type { RuntimeJobEvent } from '$lib/types';
import { baseWorkspaceState, configuredCanvasSettings } from './studentTestFixtures';

const desktopMocks = vi.hoisted(() => ({
  computeLmsBindingToken: vi.fn(),
  ensureLmsRosterPreload: vi.fn(),
  getExamWorkspaceState: vi.fn(),
  getLmsRosterCacheState: vi.fn(),
  intakeDefaultPdfRectsFromTemplate: vi.fn(),
  listCanvasCourseRoster: vi.fn(),
  priorCanonicalSubmissionExistsForLmsStudent: vi.fn(),
  toDesktopAssetUrl: vi.fn((path: string) => `asset://${path}`),
  transientClipPdfRectsPngBase64: vi.fn(),
  transientRenderPdfPagePng: vi.fn(),
  transientScansOcrHint: vi.fn()
}));

const dialogMocks = vi.hoisted(() => ({
  open: vi.fn()
}));

const webviewMocks = vi.hoisted(() => ({
  getCurrentWebview: vi.fn(() => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {})
  }))
}));

const shellMocks = vi.hoisted(() => ({
  ensureRuntimeJobBridge: vi.fn().mockResolvedValue(undefined),
  onRuntimeJobEvent: vi.fn((handler: (event: RuntimeJobEvent) => void) => {
    void handler;
    return () => {};
  })
}));

vi.mock('$lib/desktop', async () => {
  const actual = await vi.importActual<typeof import('$lib/desktop')>('$lib/desktop');
  return {
    ...actual,
    ...desktopMocks
  };
});

vi.mock('@tauri-apps/plugin-dialog', () => dialogMocks);
vi.mock('@tauri-apps/api/webview', () => webviewMocks);

vi.mock('$lib/stores/shell', async () => {
  const { writable } = await vi.importActual<typeof import('svelte/store')>('svelte/store');
  return {
    shellState: writable({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    }),
    jobProgress: writable<number | null>(null),
    ensureRuntimeJobBridge: shellMocks.ensureRuntimeJobBridge,
    onRuntimeJobEvent: shellMocks.onRuntimeJobEvent
  };
});

import StudentWorkflowWorkspace from '$lib/components/desktop/StudentWorkflowWorkspace.svelte';
import AlignmentReviewView from '$lib/components/desktop/AlignmentReviewView.svelte';

function runtimeEvent(
  eventType: RuntimeJobEvent['eventType'],
  commandName: string,
  percent: number,
  studentRef = 'student_3'
): RuntimeJobEvent {
  return {
    eventType,
    commandName,
    workerStatus: 'busy',
    requestId: 'request_1',
    jobId: 'job_1',
    payload: {
      studentRef,
      progress: { percent }
    }
  };
}

function runtimeEventWithPayload(
  eventType: RuntimeJobEvent['eventType'],
  commandName: string,
  payload: Record<string, unknown>,
  overrides: Partial<RuntimeJobEvent> = {}
): RuntimeJobEvent {
  return {
    eventType,
    commandName,
    workerStatus: 'busy',
    requestId: 'request_1',
    jobId: 'job_1',
    payload,
    ...overrides
  };
}

function requireRuntimeHandler(
  handler: ((event: RuntimeJobEvent) => void) | null
): (event: RuntimeJobEvent) => void {
  if (!handler) {
    throw new Error('Runtime event handler was not registered.');
  }
  return handler;
}

function alignmentPage(
  pageNumber: number,
  confidence: number,
  lowConfidence: boolean,
  overrides: Record<string, unknown> = {}
) {
  return {
    pageNumber,
    confidence,
    lowConfidence,
    transform: {
      rotation: 0,
      scale: 1,
      translateX: 0,
      translateY: 0
    },
    warnings: [],
    ...overrides
  };
}

function templateArtifacts(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    artifactId: `template_${index + 1}`,
    pageNumber: index + 1,
    imagePath: `/tmp/template_${index + 1}.png`,
    label: `page ${index + 1}`
  }));
}

function detectReviewRow(questionId: string, pageNumber = 1) {
  return {
    questionId,
    pageNumber,
    sourcePageImagePath: `/tmp/${questionId}_page_${pageNumber}.png`,
    templateRegion: {
      x: 10,
      y: 20,
      width: 100,
      height: 60,
      units: 'rendered_page_pixels'
    },
    warnings: [
      {
        code: 'detect_fallback',
        message: 'Question region needs review.',
        scope: `question:${questionId}`
      }
    ],
    resolvedRegion: null
  };
}

function workflowAnswer(questionNumber: number, overrides: Record<string, unknown> = {}) {
  return {
    questionId: `question_${questionNumber}`,
    questionNumber,
    cropImagePath: `/tmp/q${questionNumber}.png`,
    parseStatus: 'ok',
    parseConfidence: 'high',
    parseConfidenceSource: 'combined',
    rawParsedText: `raw answer ${questionNumber}`,
    verifiedText: `verified answer ${questionNumber}`,
    reviewRequired: false,
    verified: true,
    stale: false,
    gradingStatus: 'draft_ready',
    gradingConfidence: 'high',
    gradingConfidenceReason: null,
    questionMaxPoints: 5,
    totalPointsAwarded: 4,
    feedbackText: 'Feedback',
    criterionResults: [],
    highlights: [],
    warnings: [],
    manualGradingRequired: false,
    manualGradingReason: null,
    ...overrides
  };
}

function renderAlignmentReuseFixture(onConfirm: ReturnType<typeof vi.fn>) {
  return render(AlignmentReviewView, {
    intakeItem: {
      studentRef: 'student_1',
      canonicalPdfPath: '/tmp/student_1.pdf',
      ingestStatus: 'ok',
      pageCount: 3,
      examPagePaths: ['/tmp/student_1_p1.png', '/tmp/student_1_p2.png', '/tmp/student_1_p3.png'],
      warnings: []
    },
    submission: {
      studentRef: 'student_1',
      canonicalPdfPath: '/tmp/student_1.pdf',
      pageCount: 3,
      stage: 'alignment_review',
      latestJobId: 'job_align_1',
      failureMessage: null,
      warnings: [],
      pageArtifacts: [],
      alignmentPages: [
        alignmentPage(1, 0.33, true, { questionCount: 1 }),
        alignmentPage(2, 0.31, true, { questionCount: 1 }),
        alignmentPage(3, 0.28, true, { reviewExempt: true, questionCount: 0 })
      ],
      answers: []
    },
    templatePreviewArtifacts: templateArtifacts(3),
    displayName: 'Student One',
    onconfirm: onConfirm as unknown as (
      pages: import('$lib/types').StudentWorkflowAlignmentPage[]
    ) => Promise<void>,
    onback: vi.fn()
  });
}

describe('StudentWorkflowWorkspace review and detail panes', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    appSettings.save(configuredCanvasSettings());
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([]);
    desktopMocks.computeLmsBindingToken.mockImplementation(async (_courseId, userId: string) =>
      userId.replace('canvas_', 'token_')
    );
    desktopMocks.getLmsRosterCacheState.mockImplementation(async () => ({
      status: 'ready',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: await desktopMocks.listCanvasCourseRoster(),
      lastError: null,
      idleReason: null
    }));
    desktopMocks.ensureLmsRosterPreload.mockImplementation(async () =>
      await desktopMocks.getLmsRosterCacheState()
    );
  });

  it('renders Phase 06 workflow counts and opens alignment review from attention items', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    shellMocks.onRuntimeJobEvent.mockImplementation((handler: (event: RuntimeJobEvent) => void) => {
      runtimeHandler = handler;
      return () => {};
    });
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' },
      { userId: 'canvas_2', displayName: 'Grace Hopper', sortKey: 'hopper, grace' },
      {
        userId: 'canvas_3',
        displayName: 'Katherine Johnson',
        sortKey: 'johnson, katherine'
      },
      {
        userId: 'canvas_4',
        displayName: 'Dorothy Vaughan',
        sortKey: 'vaughan, dorothy'
      }
    ]);
    desktopMocks.computeLmsBindingToken
      .mockResolvedValueOnce('token_1')
      .mockResolvedValueOnce('token_2')
      .mockResolvedValueOnce('token_3')
      .mockResolvedValueOnce('token_4');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        questions: [
          {
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
              status: 'approved',
              criteria: [
                {
                  criterionId: 'criterion_1',
                  label: 'Accuracy',
                  points: 2,
                  partialCreditGuidance: 'Allow small arithmetic slips.',
                  source: 'manual'
                },
                {
                  criterionId: 'criterion_2',
                  label: 'Units',
                  points: 1,
                  partialCreditGuidance: 'Require correct unit notation.',
                  source: 'manual'
                },
                {
                  criterionId: 'criterion_3',
                  label: 'Reasoning',
                  points: 2,
                  partialCreditGuidance: 'Require a clear justification.',
                  source: 'manual'
                }
              ],
              warnings: [],
              approvedAt: '1',
              latestJobId: 'job_rubric_1'
            }
          }
        ],
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_1' },
          { studentRef: 'student_2', bindingTokenHex: 'token_2' },
          { studentRef: 'student_3', bindingTokenHex: 'token_3' },
          { studentRef: 'student_4', bindingTokenHex: 'token_4' }
        ],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_2_p1.png'],
              warnings: []
            },
            {
              studentRef: 'student_3',
              canonicalPdfPath: '/tmp/student_3.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_3_p1.png'],
              warnings: []
            },
            {
              studentRef: 'student_4',
              canonicalPdfPath: '/tmp/student_4.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_4_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'intake_ready',
              latestJobId: null,
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              pageCount: 4,
              stage: 'alignment_review',
              latestJobId: 'job_align_2',
              failureMessage: 'Review alignment',
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [
                {
                  pageNumber: 1,
                  confidence: 0.42,
                  lowConfidence: true,
                  transform: {
                    rotation: 0,
                    scale: 1,
                    translateX: 0,
                    translateY: 0
                  },
                  warnings: []
                },
                {
                  pageNumber: 2,
                  confidence: 0.75,
                  lowConfidence: false,
                  transform: {
                    rotation: 0,
                    scale: 1,
                    translateX: 0,
                    translateY: 0
                  },
                  warnings: []
                },
                {
                  pageNumber: 3,
                  confidence: 0.96,
                  lowConfidence: false,
                  transform: {
                    rotation: 0,
                    scale: 1,
                    translateX: 0,
                    translateY: 0
                  },
                  warnings: []
                },
                {
                  pageNumber: 4,
                  confidence: 0.38,
                  lowConfidence: true,
                  transform: {
                    rotation: 0,
                    scale: 1,
                    translateX: 0,
                    translateY: 0
                  },
                  warnings: []
                }
              ],
              answers: []
            },
            {
              studentRef: 'student_3',
              canonicalPdfPath: '/tmp/student_3.pdf',
              pageCount: 2,
              stage: 'grading',
              latestJobId: 'job_grade_3',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: '/tmp/crop_q1.png',
                  parseStatus: 'ok',
                  parseConfidence: 'high',
                  parseConfidenceSource: 'combined',
                  rawParsedText: 'raw answer',
                  verifiedText: 'verified answer',
                  reviewRequired: false,
                  verified: true,
                  stale: false,
                  gradingStatus: 'submitted',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: null,
                  feedbackText: null,
                  criterionResults: [],
                  highlights: [],
                  warnings: []
                }
              ]
            },
            {
              studentRef: 'student_4',
              canonicalPdfPath: '/tmp/student_4.pdf',
              pageCount: 2,
              stage: 'graded',
              latestJobId: 'job_grade_4',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Exam Workflow')).toBeTruthy();
    expect(await screen.findByText('Course Roster')).toBeTruthy();
    expect(screen.getByText('1 need review')).toBeTruthy();
    expect(screen.getByText('Ready to process 1 exams')).toBeTruthy();
    expect(screen.getByRole('progressbar', { name: 'Grading' }).getAttribute('aria-valuenow')).toBe('70');

    const emitRuntimeEvent = requireRuntimeHandler(runtimeHandler);
    emitRuntimeEvent(runtimeEvent('job_progress', 'grading.score-preliminary', 50));
    await waitFor(() => {
      expect(screen.getByRole('progressbar', { name: 'Grading' }).getAttribute('aria-valuenow')).toBe('79');
    });
    emitRuntimeEvent(runtimeEvent('job_progress', 'grading.score-preliminary', 80));
    await waitFor(() => {
      const v = Number(
        screen.getByRole('progressbar', { name: 'Grading' }).getAttribute('aria-valuenow')
      );
      expect(v).toBeGreaterThan(79);
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Review Unknown student' }));
    expect(await screen.findByText('42% confidence')).toBeTruthy();
    expect(screen.getByText('42%')).toBeTruthy();
    expect(screen.getByText('75%')).toBeTruthy();
    expect(screen.getByText('96%')).toBeTruthy();
    expect(screen.getByText('38%')).toBeTruthy();
    expect(screen.queryByLabelText('needs alignment review')).toBeNull();
    expect(screen.getAllByLabelText('acceptable alignment').length).toBeGreaterThan(1);
    expect(screen.getByRole('button', { name: /P2 75%/ }).parentElement?.textContent).toContain(
      '✓'
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(await screen.findByRole('dialog', { name: 'Apply transform to remaining pages?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'This Page' }));
    expect(screen.getByText('38% confidence')).toBeTruthy();
    const acceptedPageTab = screen.getByRole('button', { name: /P1 42%/ })
      .parentElement as HTMLElement;
    expect(acceptedPageTab.textContent).toContain('✓');
    await fireEvent.mouseEnter(acceptedPageTab);
    expect(acceptedPageTab.textContent).not.toContain('✓');
    await fireEvent.click(within(acceptedPageTab).getByRole('button', { name: 'Undo' }));
    expect(
      screen.getByRole('button', { name: /P1 42%/ }).parentElement?.textContent
    ).not.toContain('✓');
    await fireEvent.click(screen.getByRole('button', { name: /P1 42%/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(await screen.findByRole('dialog', { name: 'Apply transform to remaining pages?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'This Page' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(screen.getByText('38% confidence')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    expect(screen.getByRole('button', { name: /P2 75%/ }).parentElement?.textContent).toContain(
      '✓'
    );
    expect(screen.getByRole('button', { name: 'Confirm alignment → continue' }).className).toContain(
      'bg-primary'
    );
    expect(screen.getByLabelText('Template shading color')).toBeTruthy();
    expect(screen.getByText('Submission opacity · 60%')).toBeTruthy();
    expect(screen.queryByText(/Drag the submission overlay to move it/)).toBeNull();
  });

  it('opens the next actionable alignment page when the submission changes', async () => {
    const { rerender } = render(AlignmentReviewView, {
      intakeItem: {
        studentRef: 'student_1',
        canonicalPdfPath: '/tmp/student_1.pdf',
        ingestStatus: 'ok',
        pageCount: 2,
        examPagePaths: ['/tmp/student_1_p1.png', '/tmp/student_1_p2.png'],
        warnings: []
      },
      submission: {
        studentRef: 'student_1',
        canonicalPdfPath: '/tmp/student_1.pdf',
        pageCount: 2,
        stage: 'alignment_review',
        latestJobId: 'job_align_1',
        failureMessage: null,
        warnings: [],
        pageArtifacts: [],
        alignmentPages: [
          alignmentPage(1, 0.41, true, { reviewExempt: true, questionCount: 0 }),
          alignmentPage(2, 0.32, true, { questionCount: 1 })
        ],
        answers: []
      },
      templatePreviewArtifacts: templateArtifacts(2),
      displayName: 'Student One',
      onconfirm: vi.fn(),
      onback: vi.fn()
    });

    expect(await screen.findByText('32% confidence')).toBeTruthy();

    await rerender({
      intakeItem: {
        studentRef: 'student_2',
        canonicalPdfPath: '/tmp/student_2.pdf',
        ingestStatus: 'ok',
        pageCount: 3,
        examPagePaths: ['/tmp/student_2_p1.png', '/tmp/student_2_p2.png', '/tmp/student_2_p3.png'],
        warnings: []
      },
      submission: {
        studentRef: 'student_2',
        canonicalPdfPath: '/tmp/student_2.pdf',
        pageCount: 3,
        stage: 'alignment_review',
        latestJobId: 'job_align_2',
        failureMessage: null,
        warnings: [],
        pageArtifacts: [],
        alignmentPages: [
          alignmentPage(1, 0.93, false, { questionCount: 1 }),
          alignmentPage(2, 0.44, true, { reviewExempt: true, questionCount: 0 }),
          alignmentPage(3, 0.29, true, { questionCount: 2 })
        ],
        answers: []
      },
      templatePreviewArtifacts: templateArtifacts(3),
      displayName: 'Student Two',
      onconfirm: vi.fn(),
      onback: vi.fn()
    });

    expect(await screen.findByText('29% confidence')).toBeTruthy();
  });

  it('shows questionless alignment pages as exempt without accept controls', async () => {
    render(AlignmentReviewView, {
      intakeItem: {
        studentRef: 'student_1',
        canonicalPdfPath: '/tmp/student_1.pdf',
        ingestStatus: 'ok',
        pageCount: 1,
        examPagePaths: ['/tmp/student_1_p1.png'],
        warnings: []
      },
      submission: {
        studentRef: 'student_1',
        canonicalPdfPath: '/tmp/student_1.pdf',
        pageCount: 1,
        stage: 'alignment_review',
        latestJobId: 'job_align_1',
        failureMessage: null,
        warnings: [],
        pageArtifacts: [],
        alignmentPages: [
          alignmentPage(1, 0.35, true, {
            reviewExempt: true,
            reviewExemptReason: 'no_questions',
            questionCount: 0
          })
        ],
        answers: []
      },
      templatePreviewArtifacts: templateArtifacts(1),
      displayName: 'Student One',
      onconfirm: vi.fn(),
      onback: vi.fn()
    });

    expect((await screen.findAllByText('Review exempt')).length).toBeGreaterThan(0);
    expect(screen.getByText('Exempt')).toBeTruthy();
    expect(screen.getByText(/No template questions are on this page/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
  });

  it('can reuse an accepted alignment transform for remaining actionable pages', async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    renderAlignmentReuseFixture(onConfirm);

    await fireEvent.input(await screen.findByLabelText('Translate X'), { target: { value: '12' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(await screen.findByRole('dialog', { name: 'Apply transform to remaining pages?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Apply All' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm alignment → continue' }));

    expect(onConfirm).toHaveBeenCalledWith([
      expect.objectContaining({
        pageNumber: 1,
        transform: expect.objectContaining({ translateX: 12 })
      }),
      expect.objectContaining({
        pageNumber: 2,
        transform: expect.objectContaining({ translateX: 12 })
      }),
      expect.objectContaining({ pageNumber: 3, reviewExempt: true })
    ]);
  });

  it('leaves remaining alignment pages unchanged when reuse is cancelled', async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    renderAlignmentReuseFixture(onConfirm);

    await fireEvent.input(await screen.findByLabelText('Translate X'), { target: { value: '12' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(await screen.findByRole('dialog', { name: 'Apply transform to remaining pages?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'This Page' }));
    expect(screen.getByText('31% confidence')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm alignment → continue' }));

    expect(onConfirm).toHaveBeenCalledWith([
      expect.objectContaining({
        pageNumber: 1,
        transform: expect.objectContaining({ translateX: 12 })
      }),
      expect.objectContaining({
        pageNumber: 2,
        transform: expect.objectContaining({ translateX: 0 })
      }),
      expect.objectContaining({ pageNumber: 3, reviewExempt: true })
    ]);
  });

  it('can restore the auto-align transform or reset the current page to zero', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');
    const onConfirmAlignment = vi.fn().mockResolvedValue(undefined);

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        templatePreviewArtifacts: [
          {
            artifactId: 'template_1',
            pageNumber: 1,
            imagePath: '/tmp/template_1.png',
            label: 'page 1'
          }
        ],
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 1,
              stage: 'alignment_review',
              latestJobId: 'job_align_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [
                {
                  pageNumber: 1,
                  confidence: 0.5,
                  lowConfidence: true,
                  transform: {
                    rotation: 0.2,
                    scale: 0.455,
                    translateX: 8,
                    translateY: -0.3
                  },
                  warnings: []
                }
              ],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment,
      onConfirmParseReview: vi.fn()
    });

    await fireEvent.click((await screen.findAllByRole('button', { name: /^Review / }))[0]);
    await fireEvent.click(screen.getByRole('button', { name: 'Zero transform' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm alignment → continue' }));

    expect(onConfirmAlignment).toHaveBeenLastCalledWith('student_1', [
      expect.objectContaining({
        transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 }
      })
    ]);

    await fireEvent.click((await screen.findAllByRole('button', { name: /^Review / }))[0]);
    await fireEvent.click(screen.getByRole('button', { name: 'Restore auto-align' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm alignment → continue' }));

    expect(onConfirmAlignment).toHaveBeenLastCalledWith('student_1', [
      expect.objectContaining({
        transform: { rotation: 0.2, scale: 0.455, translateX: 8, translateY: -0.3 }
      })
    ]);
  });

  it('moves to the next student alignment exception after confirming alignment', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' },
      { userId: 'canvas_2', displayName: 'Grace Hopper', sortKey: 'hopper, grace' }
    ]);
    desktopMocks.computeLmsBindingToken
      .mockResolvedValueOnce('token_1')
      .mockResolvedValueOnce('token_2');
    const onConfirmAlignment = vi.fn().mockResolvedValue(undefined);

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        templatePreviewArtifacts: templateArtifacts(1),
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_1' },
          { studentRef: 'student_2', bindingTokenHex: 'token_2' }
        ],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_2_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 1,
              stage: 'alignment_review',
              latestJobId: 'job_align_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [alignmentPage(1, 0.42, true, { questionCount: 1 })],
              answers: []
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              pageCount: 1,
              stage: 'alignment_review',
              latestJobId: 'job_align_2',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [alignmentPage(1, 0.27, true, { questionCount: 1 })],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment,
      onConfirmParseReview: vi.fn()
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Review Ada Lovelace' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm alignment → continue' }));

    expect(onConfirmAlignment).toHaveBeenCalledWith('student_1', expect.any(Array));
    expect((await screen.findAllByText('Grace Hopper')).length).toBeGreaterThan(0);
    expect(screen.getByText('27% confidence')).toBeTruthy();
  });

  it('returns to the workflow board when no alignment exceptions remain after confirmation', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        templatePreviewArtifacts: templateArtifacts(1),
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 1,
              stage: 'alignment_review',
              latestJobId: 'job_align_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [alignmentPage(1, 0.42, true, { questionCount: 1 })],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn().mockResolvedValue(undefined),
      onConfirmParseReview: vi.fn()
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Review Ada Lovelace' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm alignment → continue' }));

    expect(await screen.findByText('Exam Workflow')).toBeTruthy();
    expect(screen.queryByText('42% confidence')).toBeNull();
  });

  it('moves to the next student question-region exception after confirming regions', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' },
      { userId: 'canvas_2', displayName: 'Grace Hopper', sortKey: 'hopper, grace' }
    ]);
    desktopMocks.computeLmsBindingToken
      .mockResolvedValueOnce('token_1')
      .mockResolvedValueOnce('token_2');
    const onConfirmDetectReview = vi.fn().mockResolvedValue(undefined);

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_1' },
          { studentRef: 'student_2', bindingTokenHex: 'token_2' }
        ],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_2_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 1,
              stage: 'detect_review',
              latestJobId: 'job_detect_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              detectReview: {
                pendingRows: [detectReviewRow('question_1')],
                trustedCropTargets: []
              },
              answers: []
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              pageCount: 1,
              stage: 'detect_review',
              latestJobId: 'job_detect_2',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              detectReview: {
                pendingRows: [detectReviewRow('question_2')],
                trustedCropTargets: []
              },
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmDetectReview,
      onConfirmParseReview: vi.fn()
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Review Ada Lovelace' }));
    expect(await screen.findByText('Region review')).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Region review' }).parentElement?.textContent).toBe(
      'Region review'
    );
    const warningReason = screen.getByText('Question region needs review.');
    expect(warningReason.className).toContain('line-clamp-3');
    expect(warningReason.getAttribute('title')).toBe('Question region needs review.');
    expect(warningReason.closest('button')?.textContent).toContain('question_1');
    expect(warningReason.closest('button')?.textContent).toContain('Page 1');
    expect(screen.getByRole('button', { name: 'Question region editor' }).closest('section')?.className).toBe(
      'min-h-0'
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm Regions' }));

    expect(onConfirmDetectReview).toHaveBeenCalledWith('student_1', [
      expect.objectContaining({ questionId: 'question_1' })
    ]);
    expect((await screen.findAllByText('Grace Hopper')).length).toBeGreaterThan(0);
    expect(screen.getByText('question_2')).toBeTruthy();
  });

  it('returns to the workflow board when no question-region exceptions remain after confirmation', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');
    let resolveConfirm = () => {};
    const onConfirmDetectReview = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveConfirm = resolve;
        })
    );

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 1,
              stage: 'detect_review',
              latestJobId: 'job_detect_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              detectReview: {
                pendingRows: [detectReviewRow('question_1')],
                trustedCropTargets: []
              },
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmDetectReview,
      onConfirmParseReview: vi.fn()
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Review Ada Lovelace' }));
    expect(await screen.findByText('Region review')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm Regions' }));

    expect(screen.getByText('Region review')).toBeTruthy();
    expect(screen.queryByText('Exam Workflow')).toBeNull();
    resolveConfirm();
    expect(await screen.findByText('Exam Workflow')).toBeTruthy();
    expect(screen.queryByText('Region review')).toBeNull();
  });

  it('refreshes question tabs to the next student manual-grading exception', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' },
      { userId: 'canvas_2', displayName: 'Grace Hopper', sortKey: 'hopper, grace' }
    ]);
    desktopMocks.computeLmsBindingToken
      .mockResolvedValueOnce('token_1')
      .mockResolvedValueOnce('token_2');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_1' },
          { studentRef: 'student_2', bindingTokenHex: 'token_2' }
        ],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_2_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_manual',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 1,
              stage: 'manual_grading',
              latestJobId: 'job_manual_shared',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                workflowAnswer(1, {
                  manualGradingRequired: true,
                  manualGradingReason: 'pii_detected',
                  gradingStatus: 'manual_required',
                  gradingConfidence: null,
                  feedbackText: null
                }),
                workflowAnswer(2)
              ]
            },
            {
              studentRef: 'student_2',
              canonicalPdfPath: '/tmp/student_2.pdf',
              pageCount: 1,
              stage: 'manual_grading',
              latestJobId: 'job_manual_shared',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                workflowAnswer(1, {
                  manualGradingRequired: true,
                  manualGradingReason: 'pii_detected',
                  gradingStatus: 'manual_required',
                  gradingConfidence: null,
                  feedbackText: null
                }),
                workflowAnswer(2)
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmDetectReview: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Review Ada Lovelace' }));
    await fireEvent.click(screen.getByRole('button', { name: /Q2/ }));
    expect(screen.getByRole('button', { name: /Q2/ }).textContent).toContain('✓');

    await fireEvent.click(screen.getByRole('button', { name: 'Grace Hopper Review' }));

    expect(screen.getByRole('button', { name: /Q1 manual/ }).textContent).toContain('manual');
    expect(screen.getByText('PII screening result')).toBeTruthy();
  });

  it('submits parse review corrections through the workflow callback', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');
    const onConfirmParseReview = vi.fn().mockResolvedValue(undefined);

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_2',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'parse_review',
              latestJobId: 'job_parse_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: '/tmp/crop_q1.png',
                  parseStatus: 'warning',
                  parseConfidence: 'low',
                  parseConfidenceSource: 'combined',
                  rawParsedText: 'raww answer',
                  verifiedText: 'raww answer',
                  reviewRequired: true,
                  verified: false,
                  stale: false,
                  gradingStatus: 'not_started',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: null,
                  feedbackText: null,
                  criterionResults: [],
                  highlights: [],
                  warnings: []
                }
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Review Unknown student' }));
    expect(
      await screen.findByText('low confidence OCR output needs review before grading can continue.')
    ).toBeTruthy();

    const textarea = screen.getByRole('textbox');
    await fireEvent.input(textarea, { target: { value: 'raw answer' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm answer → continue' }));

    expect(onConfirmParseReview).toHaveBeenCalledWith('student_1', 'question_1', 'raw answer');
  });

  it('resets the parse-review draft when the next pending answer becomes active', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    const initialWorkspaceState = baseWorkspaceState({
      studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
      studentIntake: {
        status: 'ready',
        latestJobId: null,
        unresolvedCount: 0,
        items: [
          {
            studentRef: 'student_1',
            canonicalPdfPath: '/tmp/student_1.pdf',
            ingestStatus: 'ok',
            pageCount: 2,
            examPagePaths: ['/tmp/student_1_p1.png'],
            warnings: []
          }
        ]
      },
      studentWorkflow: {
        status: 'attention',
        latestJobId: 'job_workflow_2',
        submissions: [
          {
            studentRef: 'student_1',
            canonicalPdfPath: '/tmp/student_1.pdf',
            pageCount: 2,
            stage: 'parse_review',
            latestJobId: 'job_parse_1',
            failureMessage: null,
            warnings: [],
            pageArtifacts: [],
            alignmentPages: [],
            answers: [
              {
                questionId: 'question_1',
                questionNumber: 1,
                cropImagePath: '/tmp/crop_q1.png',
                parseStatus: 'warning',
                parseConfidence: 'low',
                parseConfidenceSource: 'combined',
                rawParsedText: 'raw answer one',
                verifiedText: 'raw answer one',
                reviewRequired: true,
                verified: false,
                stale: false,
                gradingStatus: 'not_started',
                gradingConfidence: null,
                gradingConfidenceReason: null,
                questionMaxPoints: 5,
                totalPointsAwarded: null,
                feedbackText: null,
                criterionResults: [],
                highlights: [],
                warnings: []
              },
              {
                questionId: 'question_2',
                questionNumber: 2,
                cropImagePath: '/tmp/crop_q2.png',
                parseStatus: 'warning',
                parseConfidence: 'low',
                parseConfidenceSource: 'combined',
                rawParsedText: 'raw answer two',
                verifiedText: 'raw answer two',
                reviewRequired: true,
                verified: false,
                stale: false,
                gradingStatus: 'not_started',
                gradingConfidence: null,
                gradingConfidenceReason: null,
                questionMaxPoints: 5,
                totalPointsAwarded: null,
                feedbackText: null,
                criterionResults: [],
                highlights: [],
                warnings: []
              }
            ]
          }
        ]
      }
    });

    const view = render(StudentWorkflowWorkspace, {
      workspaceState: initialWorkspaceState,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Review Unknown student' }));
    expect(
      await screen.findByText('low confidence OCR output needs review before grading can continue.')
    ).toBeTruthy();

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: 'edited first answer' } });
    expect(textarea.value).toBe('edited first answer');

    await view.rerender({
      workspaceState: baseWorkspaceState({
        ...initialWorkspaceState,
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_2',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'parse_review',
              latestJobId: 'job_parse_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: '/tmp/crop_q1.png',
                  parseStatus: 'ok',
                  parseConfidence: 'medium',
                  parseConfidenceSource: 'combined',
                  rawParsedText: 'raw answer one',
                  verifiedText: 'edited first answer',
                  reviewRequired: false,
                  verified: true,
                  stale: false,
                  gradingStatus: 'not_started',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: null,
                  feedbackText: null,
                  criterionResults: [],
                  highlights: [],
                  warnings: []
                },
                {
                  questionId: 'question_2',
                  questionNumber: 2,
                  cropImagePath: '/tmp/crop_q2.png',
                  parseStatus: 'warning',
                  parseConfidence: 'low',
                  parseConfidenceSource: 'combined',
                  rawParsedText: 'raw answer two',
                  verifiedText: 'raw answer two',
                  reviewRequired: true,
                  verified: false,
                  stale: false,
                  gradingStatus: 'not_started',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: null,
                  feedbackText: null,
                  criterionResults: [],
                  highlights: [],
                  warnings: []
                }
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    await waitFor(() => {
      expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('raw answer two');
    });
    expect(screen.getByRole('button', { name: /Q2/ })).toBeTruthy();
  });

  it('shows parsed answer, grading summary, and feedback in student detail', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'graded',
          latestJobId: 'job_workflow_graded',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'graded',
              latestJobId: 'job_grade_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: '/tmp/crop_q1.png',
                  parseStatus: 'ok',
                  parseConfidence: 'high',
                  parseConfidenceSource: 'combined',
                  rawParsedText: 'raw answer text',
                  verifiedText: 'verified answer text',
                  reviewRequired: false,
                  verified: true,
                  stale: false,
                  gradingStatus: 'draft_ready',
                  gradingConfidence: 'medium',
                  gradingConfidenceReason:
                    'One rubric criterion needed a degraded fallback.',
                  questionMaxPoints: 5,
                  totalPointsAwarded: 4,
                  feedbackText: 'Strong explanation, but cite one more supporting detail.',
                  criterionResults: [
                    {
                      criterionIndex: 0,
                      label: 'Core event',
                      points: 2,
                      pointsAwarded: 2,
                      rationale: 'Covered the core historical event.'
                    },
                    {
                      criterionIndex: 1,
                      label: 'Supporting detail',
                      points: 3,
                      pointsAwarded: 2,
                      rationale: 'Missed one supporting detail.'
                    }
                  ],
                  highlights: [],
                  warnings: []
                }
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await fireEvent.click(within(sidebar).getByRole('button', { name: /Unknown student/i }));

    expect(await screen.findByText('Criterion results')).toBeTruthy();
    expect(screen.getByText('verified answer text')).toBeTruthy();
    expect(screen.getByText('Strong explanation, but cite one more supporting detail.')).toBeTruthy();
    expect(screen.getByText('Core event')).toBeTruthy();
    expect(screen.getByText('Supporting detail')).toBeTruthy();
  });

  it('keeps parse progress moving across internal CLI substages', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    shellMocks.onRuntimeJobEvent.mockImplementation((handler: (event: RuntimeJobEvent) => void) => {
      runtimeHandler = handler;
      return () => {};
    });
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'running',
          latestJobId: 'job_workflow_parse',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'parse',
              latestJobId: 'job_parse_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    expect(screen.getByRole('progressbar', { name: 'Parsing' }).getAttribute('aria-valuenow')).toBe('55');

    const emitRuntimeEvent = requireRuntimeHandler(runtimeHandler);
    emitRuntimeEvent(
      runtimeEventWithPayload(
        'job_progress',
        'scans.parse',
        {
          progress: { percent: 0 },
          scope: { student_ref: 'student_1' },
          event: 'started',
          data: { total_stages: 2 }
        },
        { jobId: 'job_parse_1' }
      )
    );
    emitRuntimeEvent(
      runtimeEventWithPayload(
        'job_progress',
        'scans.parse',
        {
          event: 'stage_started',
          data: { stage_number: 2 }
        },
        { jobId: 'job_parse_1' }
      )
    );
    emitRuntimeEvent(
      runtimeEventWithPayload(
        'job_progress',
        'scans.parse',
        {
          progress: { percent: 0 },
          scope: { student_ref: 'student_1' },
          event: 'item_started',
          data: { stage: 'parse_ocr' }
        },
        { jobId: 'job_parse_1' }
      )
    );

    await waitFor(() => {
      expect(screen.getByRole('progressbar', { name: 'Parsing' }).getAttribute('aria-valuenow')).toBe('63');
    });
  });

  it('does not reset grading bar to stage baseline when workflow_state_updated repeats the same stage', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    shellMocks.onRuntimeJobEvent.mockImplementation((handler: (event: RuntimeJobEvent) => void) => {
      runtimeHandler = handler;
      return () => {};
    });
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'running',
          latestJobId: 'job_wf',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'grading',
              latestJobId: 'job_prelim',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: '/tmp/crop.png',
                  parseStatus: 'ok',
                  parseConfidence: 'high',
                  parseConfidenceSource: 'combined',
                  rawParsedText: 'x',
                  verifiedText: 'x',
                  reviewRequired: false,
                  verified: true,
                  stale: false,
                  gradingStatus: 'submitted',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: null,
                  feedbackText: null,
                  criterionResults: [],
                  highlights: [],
                  warnings: []
                }
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    const bar = () => screen.getByRole('progressbar', { name: 'Grading' });
    expect(bar().getAttribute('aria-valuenow')).toBe('70');

    const emitRuntimeEvent = requireRuntimeHandler(runtimeHandler);
    emitRuntimeEvent(
      runtimeEventWithPayload(
        'job_finished',
        'grading.score-preliminary',
        { studentRef: 'student_1' },
        { jobId: 'job_prelim' }
      )
    );
    await waitFor(() => {
      expect(bar().getAttribute('aria-valuenow')).toBe('80');
    });

    emitRuntimeEvent(
      runtimeEventWithPayload('workflow_state_updated', 'student.workflow', {
        workflowStateUpdated: true,
        studentRef: 'student_1',
        stage: 'grading',
        workflowStatus: 'running'
      })
    );
    expect(bar().getAttribute('aria-valuenow')).toBe('80');
  });

  it('advances grading progress through several distinct bar values for multi-criterion runs', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    shellMocks.onRuntimeJobEvent.mockImplementation((handler: (event: RuntimeJobEvent) => void) => {
      runtimeHandler = handler;
      return () => {};
    });
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    const criterion = (id: string) => ({
      criterionId: id,
      label: id,
      points: 1,
      partialCreditGuidance: '',
      source: 'test'
    });

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        questions: [
          {
            questionId: 'question_1',
            questionNumber: 1,
            pageNumber: 1,
            maxPoints: 5,
            text: 'Q1',
            baselinePdfText: 'Q1',
            sourceArtifactId: null,
            analysis: {
              status: 'ok',
              questionTextClean: 'Q1',
              questionContext: '',
              warnings: [],
              latestJobId: 'job_a'
            },
            rubric: {
              status: 'ok',
              criteria: [criterion('c1'), criterion('c2'), criterion('c3')],
              warnings: [],
              approvedAt: null,
              latestJobId: null
            }
          }
        ],
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'running',
          latestJobId: 'job_wf',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'grading',
              latestJobId: 'job_grade_multi',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: '/tmp/crop.png',
                  parseStatus: 'ok',
                  parseConfidence: 'high',
                  parseConfidenceSource: 'combined',
                  rawParsedText: 'x',
                  verifiedText: 'x',
                  reviewRequired: false,
                  verified: true,
                  stale: false,
                  gradingStatus: 'submitted',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: null,
                  feedbackText: null,
                  criterionResults: [],
                  highlights: [],
                  warnings: []
                }
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    const bar = () => screen.getByRole('progressbar', { name: 'Grading' });
    const emitRuntimeEvent = requireRuntimeHandler(runtimeHandler);

    const expectBar = async (expected: string) => {
      await waitFor(() => {
        expect(bar().getAttribute('aria-valuenow')).toBe(expected);
      });
    };

    const emitGradingProgress = (percent: number) =>
      emitRuntimeEvent(
        runtimeEventWithPayload(
          'job_progress',
          'grading.score-preliminary',
          {
            studentRef: 'student_1',
            progress: { percent },
            event: 'item_completed',
            data: {}
          },
          { jobId: 'job_grade_multi' }
        )
      );

    emitGradingProgress(12);
    await expectBar('72');
    emitGradingProgress(45);
    await expectBar('78');
    emitGradingProgress(88);
    await expectBar('85');
  });

  it('moves parse bar on stage_started alone after started establishes multiple CLI stages', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    shellMocks.onRuntimeJobEvent.mockImplementation((handler: (event: RuntimeJobEvent) => void) => {
      runtimeHandler = handler;
      return () => {};
    });
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'running',
          latestJobId: 'job_workflow_parse',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'parse',
              latestJobId: 'job_parse_sub',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    const bar = () => screen.getByRole('progressbar', { name: 'Parsing' });
    expect(bar().getAttribute('aria-valuenow')).toBe('55');

    const emitRuntimeEvent = requireRuntimeHandler(runtimeHandler);
    emitRuntimeEvent(
      runtimeEventWithPayload(
        'job_progress',
        'scans.parse',
        {
          progress: { percent: 0 },
          studentRef: 'student_1',
          event: 'started',
          data: { total_stages: 2 }
        },
        { jobId: 'job_parse_sub' }
      )
    );
    expect(bar().getAttribute('aria-valuenow')).toBe('55');

    emitRuntimeEvent(
      runtimeEventWithPayload(
        'job_progress',
        'scans.parse',
        {
          event: 'stage_started',
          data: { stage_number: 2 }
        },
        { jobId: 'job_parse_sub' }
      )
    );
    await waitFor(() => {
      expect(bar().getAttribute('aria-valuenow')).toBe('63');
    });
  });

  it('shows manual-only answers alongside graded answers when a submission enters manual grading', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_1');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_1' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_manual',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'manual_grading',
              latestJobId: 'job_pii_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: '/tmp/q1.png',
                  parseStatus: 'blocked',
                  parseConfidence: null,
                  parseConfidenceSource: null,
                  rawParsedText: null,
                  verifiedText: null,
                  reviewRequired: false,
                  verified: false,
                  stale: false,
                  gradingStatus: 'manual_required',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: 0,
                  feedbackText: null,
                  criterionResults: [
                    {
                      criterionIndex: 0,
                      label: 'Main point',
                      points: 2,
                      pointsAwarded: 0,
                      rationale: ''
                    },
                    {
                      criterionIndex: 1,
                      label: 'Supporting detail',
                      points: 3,
                      pointsAwarded: 0,
                      rationale: ''
                    }
                  ],
                  highlights: [],
                  warnings: [
                    {
                      code: 'pii_name_detected',
                      message: 'Student name detected in the cropped response.',
                      scope: null
                    }
                  ],
                  piiPrescreen: {
                    sourceCommand: 'scans.pii',
                    status: 'ok',
                    containsHandwriting: 'true',
                    containsPii: true,
                    piiTypesDetected: ['name'],
                    warnings: []
                  },
                  manualGradingRequired: true,
                  manualGradingReason: 'pii_detected'
                },
                {
                  questionId: 'question_2',
                  questionNumber: 2,
                  cropImagePath: '/tmp/q2.png',
                  parseStatus: 'ok',
                  parseConfidence: 'high',
                  parseConfidenceSource: 'pii_prescreen',
                  rawParsedText: 'raw answer text',
                  verifiedText: 'verified answer text',
                  reviewRequired: false,
                  verified: true,
                  stale: false,
                  gradingStatus: 'draft_ready',
                  gradingConfidence: 'high',
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: 4,
                  feedbackText: 'Solid explanation.',
                  criterionResults: [],
                  highlights: [],
                  warnings: [],
                  piiPrescreen: {
                    sourceCommand: 'scans.pii',
                    status: 'ok',
                    containsHandwriting: 'true',
                    containsPii: false,
                    piiTypesDetected: [],
                    warnings: []
                  },
                  manualGradingRequired: false,
                  manualGradingReason: null
                }
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await fireEvent.click(within(sidebar).getByRole('button', { name: /Unknown student/i }));

    expect(screen.getByRole('progressbar', { name: 'manual grading' }).getAttribute('aria-valuenow')).toBe('99');
    expect(screen.getByRole('button', { name: /q1/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /q2/i })).toBeTruthy();
    expect(screen.getByText('manual')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: /q1/i }));
    expect(
      screen.getAllByText(
        'Student PII was detected in this answer. Parse, grading, feedback, and markup were skipped.'
      ).length
    ).toBeGreaterThan(0);
    expect(screen.getByText('Rubric scoring')).toBeTruthy();
    expect(screen.getByText('Main point')).toBeTruthy();
    expect(screen.getByText('handwriting detected')).toBeTruthy();
    expect(screen.getByText('Detected PII: name')).toBeTruthy();
    expect(screen.getByText('Student name detected in the cropped response.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: /q2/i }));
    expect(screen.getByText('verified answer text')).toBeTruthy();
    expect(screen.getByText('Solid explanation.')).toBeTruthy();
  });

  it('renders crop failures as distinct manual blocks', async () => {
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        },
        studentWorkflow: {
          status: 'attention',
          latestJobId: 'job_workflow_manual_crop',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 1,
              stage: 'manual_grading',
              latestJobId: 'job_crop_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: [
                {
                  questionId: 'question_1',
                  questionNumber: 1,
                  cropImagePath: null,
                  parseStatus: 'blocked',
                  parseConfidence: null,
                  parseConfidenceSource: null,
                  rawParsedText: null,
                  verifiedText: null,
                  reviewRequired: false,
                  verified: false,
                  stale: false,
                  gradingStatus: 'manual_required',
                  gradingConfidence: null,
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: null,
                  feedbackText: null,
                  criterionResults: [],
                  highlights: [],
                  warnings: [
                    {
                      code: 'crop_failed',
                      message: 'Question crop generation failed.',
                      scope: 'answer'
                    }
                  ],
                  piiPrescreen: null,
                  manualGradingRequired: true,
                  manualGradingReason: 'crop_failed'
                }
              ]
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow: vi.fn(),
      onConfirmAlignment: vi.fn(),
      onConfirmParseReview: vi.fn()
    });

    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await fireEvent.click(within(sidebar).getByRole('button', { name: /Unknown student/i }));
    await fireEvent.click(screen.getByRole('button', { name: /q1/i }));

    expect(
      screen.getAllByText(
        'Question cropping failed for this answer. PII screening, parse, grading, feedback, and markup were skipped.'
      ).length
    ).toBeGreaterThan(0);
    expect(
      screen.getByText('No PII prescreen data was recorded because the crop did not complete cleanly.')
    ).toBeTruthy();
    expect(screen.getByText('Question crop generation failed.')).toBeTruthy();
  });
});
