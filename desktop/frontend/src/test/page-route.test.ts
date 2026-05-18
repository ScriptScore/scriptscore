// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

  const desktopMocks = vi.hoisted(() => ({
    approveTemplateSetup: vi.fn(),
    ensureAutomaticRubricJobs: vi.fn().mockResolvedValue(undefined),
    ensureLmsRosterPreload: vi.fn().mockResolvedValue({
      status: 'ready',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: [],
      lastError: null,
      idleReason: null
    }),
    closeCurrentProject: vi.fn(),
    computeLmsBindingToken: vi.fn(),
    createProject: vi.fn(),
    beginStudentWorkflow: vi.fn(),
    confirmStudentAlignment: vi.fn(),
    confirmStudentDetectReview: vi.fn(),
    confirmStudentParseReview: vi.fn(),
    cancelActiveJob: vi.fn(),
    recoverInterruptedStudentWorkflow: vi.fn(),
    generateQuestionRubric: vi.fn(),
    getJobTrace: vi.fn(),
    getLmsRosterCacheState: vi.fn().mockResolvedValue({
      status: 'ready',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: [],
      lastError: null,
      idleReason: null
    }),
    getShellState: vi.fn(),
    getExamWorkspaceState: vi.fn(),
    isDesktopHost: vi.fn(),
    listCanvasCourses: vi.fn(),
    listCanvasCourseRoster: vi.fn(),
    listJobTraces: vi.fn(),
    listLmsAssignments: vi.fn().mockResolvedValue([]),
    listLmsAssignmentsForCourse: vi.fn().mockResolvedValue([]),
    listVisionModels: vi.fn(),
    listenRuntimeJobEvents: vi.fn(),
    openProject: vi.fn(),
    previewResultsLmsReport: vi.fn().mockResolvedValue({
      studentRef: 'student_1',
      resultFingerprint: 'fp_1',
      html: '<!doctype html><html><body>Report</body></html>'
    }),
    retryResultsLmsUpload: vi.fn(),
    reanalyzeQuestion: vi.fn(),
    exportStampedTemplatePdf: vi.fn(),
    replaceTemplatePdf: vi.fn(),
  finalizeReadyResults: vi.fn(),
  runSmokePing: vi.fn(),
  runResultsLmsUpload: vi.fn(),
  runResultsExport: vi.fn(),
  saveQuestionEdits: vi.fn(),
  saveProjectConfig: vi.fn(),
  saveResultsLmsAssignment: vi.fn(),
  saveRubricUpdate: vi.fn(),
  saveRedactionRegions: vi.fn(),
  saveStudentAlignmentReview: vi.fn(),
  saveStudentDetectReview: vi.fn(),
  saveStudentParseReview: vi.fn(),
  saveModeratedScore: vi.fn(),
  setSubmissionResultFinalized: vi.fn(),
  skipTemplateRedaction: vi.fn(),
  setModerationQuestionReviewed: vi.fn(),
  resolveLmsStudentRef: vi.fn(),
  runStudentIntake: vi.fn()
}));

const dialogMocks = vi.hoisted(() => ({
  confirm: vi.fn(),
  open: vi.fn(),
  save: vi.fn()
}));

vi.mock('$lib/desktop', async () => {
  const actual = await vi.importActual<typeof import('$lib/desktop')>('$lib/desktop');
  return {
    ...actual,
    ...desktopMocks
  };
});

vi.mock('@tauri-apps/plugin-dialog', () => dialogMocks);

import Page from '../routes/+page.svelte';
import { appSettings, defaultAppSettings } from '$lib/stores/appSettings';
import { shellState, teardownRuntimeJobBridge } from '$lib/stores/shell';
import { notifications } from '$lib/stores/notifications';
import { workspaceView } from '$lib/stores/workspaceView';
import { resultsWorkspaceView } from '$lib/stores/resultsWorkspaceView';
import type { ExamWorkspaceState, RuntimeJobEvent, ShellState } from '$lib/types';

function projectSummary() {
  return {
    projectId: 'proj_1',
    displayName: 'Midterm 1',
    subject: 'Physics',
    courseCode: 'PHYS 221',
    lmsCourseId: null,
    projectPath: '/tmp/midterm-1',
    createdAt: '1',
    updatedAt: '1'
  };
}

function workspaceState(overrides: Partial<ExamWorkspaceState> = {}): ExamWorkspaceState {
  const state: ExamWorkspaceState = {
    project: projectSummary(),
    status: 'draft',
    statusLabel: 'Question review in progress',
    failureMessage: null,
    templatePreviewArtifacts: [
      {
        artifactId: 'artifact_page_1',
        pageNumber: 1,
        imagePath: '/tmp/page-1.png',
        label: 'Page 1'
      }
    ],
    questions: [
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Explain westward expansion.',
        baselinePdfText: 'Explain westward expansion.',
        sourceArtifactId: 'artifact_page_1'
      }
    ],
    redactionRegions: [],
    warnings: [],
    canApprove: true,
    canApproveRubric: true,
    projectConfig: {
      projectId: 'proj_test',
      displayName: 'Midterm 1',
      subject: 'Physics',
      courseCode: 'PHYS 221',
      lmsCourseId: null,
      redactionRequired: true,
      instructorProfile: {
        gradingStrictness: 'balanced',
        syntaxLeniency: 'medium',
        ocrTolerance: 'medium',
        partialCreditStyle: 'balanced',
        feedbackStyle: 'brief',
        enabledTags: {
          gradingStrictness: true,
          syntaxLeniency: false,
          ocrTolerance: false,
          partialCreditStyle: false,
          feedbackStyle: true
        },
        additionalGuidance: '',
        includeMinimumCreditCriterion: false,
        minimumCreditPercent: 10
      },
      traceRefs: { setupJobId: null, batchAnalyzeJobId: null, batchRubricJobId: null, intakeJobId: null },
      createdAt: '1',
      updatedAt: '1'
    },
    workflowStage: 'question_review',
    workflowLabel: 'Question review in progress'
  };
  return { ...state, ...overrides };
}

function configuredCanvasSettings() {
  return {
    ...defaultAppSettings,
    lmsProvider: 'canvas' as const,
    lmsCanvasBaseUrl: 'https://canvas.example.test',
    lmsCanvasApiKey: 'token'
  };
}

function multiQuestionWorkspaceState(): ExamWorkspaceState {
  return {
    ...workspaceState(),
    templatePreviewArtifacts: [
      {
        artifactId: 'artifact_page_1',
        pageNumber: 1,
        imagePath: '/tmp/page-1.png',
        label: 'Page 1'
      },
      {
        artifactId: 'artifact_page_2',
        pageNumber: 2,
        imagePath: '/tmp/page-2.png',
        label: 'Page 2'
      }
    ],
    questions: [
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Explain westward expansion.',
        baselinePdfText: 'Explain westward expansion.',
        sourceArtifactId: 'artifact_page_1'
      },
      {
        questionId: 'question_2',
        questionNumber: 2,
        pageNumber: 2,
        maxPoints: 8,
        text: 'Explain orbital hybridization.',
        baselinePdfText: 'Explain orbital hybridization.',
        sourceArtifactId: 'artifact_page_2'
      }
    ],
    redactionRegions: [
      {
        regionId: 'region_1',
        pageNumber: 1,
        x: 10,
        y: 10,
        width: 30,
        height: 30,
        label: 'name_identification',
        sortOrder: 0
      }
    ],
    canApprove: true
  };
}

function analyzedWorkspaceState(): ExamWorkspaceState {
  return {
    ...workspaceState(),
    questions: [
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Cleaned question text.',
        baselinePdfText: 'Explain westward expansion.',
        sourceArtifactId: 'artifact_page_1',
        analysis: {
          status: 'ok',
          questionTextClean: 'Cleaned question text.',
          questionContext: '',
          warnings: [],
          latestJobId: 'job_analyze_1'
        }
      }
    ]
  };
}

function resultsUploadWorkspaceState(): ExamWorkspaceState {
  return {
    ...workspaceState({
      workflowStage: 'results_upload_ready',
      workflowLabel: 'Ready for Results',
      project: {
        ...projectSummary(),
        lmsCourseId: 'course_1'
      }
    }),
    projectConfig: {
      ...workspaceState().projectConfig!,
      lmsCourseId: 'course_1',
      lmsAssignmentId: 'assignment_1'
    },
    resultsLmsState: {
      selectedTarget: {
        provider: 'canvas',
        courseId: 'course_1',
        assignmentId: 'assignment_1'
      },
      finalizationRecords: [],
      uploadAttempts: []
    },
    resultsLmsRows: [
      {
        studentRef: 'student_1',
        aggregateTotal: 96,
        aggregateComplete: true,
        readyToFinalize: true,
        blockedReasons: [],
        questionRows: [],
        resultFingerprint: 'fp_1',
        finalized: true,
        staleFinalization: false,
        finalizedAt: '1',
        uploaded: false,
        uploadFailed: false,
        latestUploadError: null,
        lastUploadAttemptId: null
      }
    ],
    resultsLmsMetrics: {
      scoredStudentCount: 1,
      averageScore: 96,
      medianScore: 96,
      minScore: 96,
      maxScore: 96,
      questionMetrics: []
    },
    resultsLmsReviewSummary: {
      totalReviewableQuestions: 0,
      unreviewedQuestionCount: 0,
      hasUnreviewedQuestions: false
    }
  };
}

function studentsWorkflowWorkspaceState(
  stage: string = 'intake_ready'
): ExamWorkspaceState {
  return workspaceState({
    status: 'approved',
    statusLabel: 'Ready for student intake',
    studentIntake: {
      status: 'ok',
      latestJobId: 'job_intake_1',
      unresolvedCount: 0,
      items: [
        {
          studentRef: 'student_1',
          canonicalPdfPath: '/tmp/student_1.pdf',
          ingestStatus: 'ok',
          pageCount: 2,
          examPagePaths: ['/tmp/student_1_p1.png'],
          warnings: [],
          bindingTokenHex: 'token_canvas_1'
        }
      ]
    },
    studentRoster: [
      {
        studentRef: 'student_1',
        bindingTokenHex: 'token_canvas_1'
      }
    ],
    studentWorkflow: {
      status: stage === 'intake_ready' ? 'ready' : 'running',
      latestJobId: 'job_workflow_1',
      submissions: [
        {
          studentRef: 'student_1',
          canonicalPdfPath: '/tmp/student_1.pdf',
          pageCount: 2,
          stage,
          latestJobId: 'job_stage_1',
          failureMessage: null,
          warnings: [],
          pageArtifacts: [],
          alignmentPages: [],
          answers: []
        }
      ]
    },
    projectConfig: {
      ...(workspaceState().projectConfig!),
      lmsCourseId: 'persisted-course-id'
    }
  });
}

function studentWorkflowReviewWorkspaceState(
  stage: 'alignment_review' | 'detect_review' | 'parse_review'
): ExamWorkspaceState {
  return workspaceState({
    status: 'approved',
    statusLabel: 'Student workflow review',
    workflowStage: 'student_workflow_review',
    workflowLabel: 'Student workflow review',
    templatePreviewArtifacts: [
      {
        artifactId: 'template_page_1',
        pageNumber: 1,
        imagePath: '/tmp/template_1.png',
        label: 'Page 1'
      }
    ],
    studentIntake: {
      status: 'ok',
      latestJobId: 'job_intake_1',
      unresolvedCount: 0,
      items: [
        {
          studentRef: 'student_1',
          canonicalPdfPath: '/tmp/student_1.pdf',
          ingestStatus: 'ok',
          pageCount: 1,
          examPagePaths: ['/tmp/student_1_p1.png'],
          warnings: [],
          bindingTokenHex: 'token_canvas_1'
        }
      ]
    },
    studentRoster: [
      {
        studentRef: 'student_1',
        bindingTokenHex: 'token_canvas_1'
      }
    ],
    studentWorkflow: {
      status: 'attention',
      latestJobId: 'job_workflow_1',
      submissions: [
        {
          studentRef: 'student_1',
          canonicalPdfPath: '/tmp/student_1.pdf',
          pageCount: 1,
          stage,
          latestJobId: 'job_stage_1',
          failureMessage: null,
          warnings: [],
          pageArtifacts: [],
          alignmentPages:
            stage === 'alignment_review'
              ? [
                  {
                    pageNumber: 1,
                    confidence: 0.99,
                    lowConfidence: false,
                    reviewExempt: false,
                    reviewExemptReason: null,
                    questionCount: 1,
                    transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 },
                    warnings: []
                  }
                ]
              : [],
          detectReview:
            stage === 'detect_review'
              ? {
                  pendingRows: [
                    {
                      questionId: 'question_1',
                      pageNumber: 1,
                      sourcePageImagePath: '/tmp/student_1_p1.png',
                      templateRegion: {
                        x: 10,
                        y: 10,
                        width: 100,
                        height: 40,
                        units: 'rendered_page_pixels'
                      },
                      warnings: [],
                      resolvedRegion: null
                    }
                  ],
                  trustedCropTargets: []
                }
              : undefined,
          answers:
            stage === 'parse_review'
              ? [
                  {
                    questionId: 'question_1',
                    questionNumber: 1,
                    cropImagePath: '/tmp/q1.png',
                    piiPrescreen: null,
                    manualGradingRequired: false,
                    manualGradingReason: null,
                    moderationEligible: true,
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
              : []
        }
      ]
    },
    projectConfig: {
      ...(workspaceState().projectConfig!),
      lmsCourseId: 'persisted-course-id'
    }
  });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function runtimeJobEvent(
  overrides: Partial<RuntimeJobEvent> = {}
): RuntimeJobEvent {
  return {
    eventType: 'job_finished',
    commandName: 'create_project',
    workerStatus: 'ready',
    requestId: 'req_1',
    jobId: 'job_1',
    payload: {},
    ...overrides
  };
}

function hostWorkflowPayload(
  resultKind: 'workspace' | 'shell',
  data: unknown,
  workspaceChanged = true
): Record<string, unknown> {
  return {
    resultKind,
    data,
    workspaceChanged,
    nestedJobIds: []
  };
}

async function openCreateProjectForm(): Promise<void> {
  await fireEvent.click(screen.getAllByRole('button', { name: 'Create Project' })[0]);
  expect(await screen.findByRole('heading', { name: 'Create a Project' })).toBeTruthy();
}

async function populateCreateProjectForm(): Promise<void> {
  await fireEvent.input(screen.getByLabelText('Exam Name'), {
    target: { value: 'Midterm 1' }
  });
  dialogMocks.open.mockResolvedValue('/tmp/midterm-1/template.pdf');
  await fireEvent.click(screen.getByRole('button', { name: 'Choose PDF' }));
  expect(await screen.findByText('/tmp/midterm-1/template.pdf')).toBeTruthy();
}

async function selectTemplateSetupSubstep(label: 'Setup' | 'Review'): Promise<void> {
  const trigger = screen.getByRole('button', { name: 'Template' });
  await fireEvent.focus(trigger);
  await fireEvent.click(await screen.findByRole('menuitem', { name: label }));
}

/** Matches `scheduleAutomaticRubricEnsureAfterUiPaint`: tick + double rAF + microtask. */
async function flushScheduledAutomaticRubricEnsure(): Promise<void> {
  await tick();
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        queueMicrotask(resolve);
      });
    });
  });
  await Promise.resolve();
}

describe('desktop route shell', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    globalThis.localStorage.clear();
    shellState.set({
      currentProject: null,
      workerStatus: 'starting',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    notifications.clear();
    teardownRuntimeJobBridge();
    workspaceView.reset();
    resultsWorkspaceView.reset();
    desktopMocks.approveTemplateSetup.mockResolvedValue(workspaceState());
    desktopMocks.closeCurrentProject.mockResolvedValue({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.createProject.mockResolvedValue('job_1');
    desktopMocks.openProject.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.replaceTemplatePdf.mockResolvedValue('job_replace_1');
    desktopMocks.runSmokePing.mockResolvedValue({
      command: 'smoke.ping',
      message: 'desktop runtime ready',
      steps: 1,
      eventCount: 1
    });
    desktopMocks.saveProjectConfig.mockResolvedValue(workspaceState());
    desktopMocks.saveResultsLmsAssignment.mockResolvedValue(workspaceState());
    desktopMocks.saveRubricUpdate.mockResolvedValue(workspaceState());
    desktopMocks.generateQuestionRubric.mockResolvedValue('job_rubric_1');
    desktopMocks.getJobTrace.mockResolvedValue(null);
    desktopMocks.listVisionModels.mockResolvedValue([
      {
        name: 'qwen2.5vl:7b',
        displayName: 'qwen2.5vl:7b'
      },
      {
        name: 'llava:7b',
        displayName: 'llava:7b'
      }
    ]);
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      {
        userId: 'canvas_1',
        displayName: 'Jordan Rivera',
        sortKey: 'rivera, jordan'
      }
    ]);
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
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_canvas_1');
    desktopMocks.beginStudentWorkflow.mockResolvedValue('job_student_workflow_1');
    desktopMocks.confirmStudentAlignment.mockResolvedValue('job_confirm_alignment_1');
    desktopMocks.confirmStudentDetectReview.mockResolvedValue('job_confirm_detect_1');
    desktopMocks.confirmStudentParseReview.mockResolvedValue('job_confirm_parse_1');
    desktopMocks.cancelActiveJob.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      workerActivity: { activeJobs: [], pendingJobCount: 0 },
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.recoverInterruptedStudentWorkflow.mockResolvedValue(studentsWorkflowWorkspaceState('stopped'));
    desktopMocks.runStudentIntake.mockResolvedValue('job_intake_1');
    desktopMocks.resolveLmsStudentRef.mockResolvedValue({
      studentRef: 'student_1',
      bindingTokenHex:
        'deadbeef0000000000000000000000000000000000000000000000000000000000'
    });
    desktopMocks.saveQuestionEdits.mockResolvedValue(workspaceState());
    desktopMocks.saveRedactionRegions.mockResolvedValue(workspaceState());
    desktopMocks.saveStudentAlignmentReview.mockResolvedValue(studentsWorkflowWorkspaceState('canonicalize'));
    desktopMocks.saveStudentDetectReview.mockResolvedValue(studentsWorkflowWorkspaceState('crop'));
    desktopMocks.saveStudentParseReview.mockResolvedValue(studentsWorkflowWorkspaceState('grading'));
    desktopMocks.skipTemplateRedaction.mockResolvedValue(
      workspaceState({
        statusLabel: 'Redaction skipped, review questions',
        warnings: [
          {
            code: 'redaction_skipped',
            message:
              'Redaction was skipped. Student-identifying regions will not be masked unless you add them before approval.',
            scope: null
          }
        ],
        canApprove: true
      })
    );
    desktopMocks.listenRuntimeJobEvents.mockResolvedValue(vi.fn());
    desktopMocks.listCanvasCourses.mockResolvedValue([]);
    desktopMocks.listJobTraces.mockResolvedValue([]);
    desktopMocks.listLmsAssignmentsForCourse.mockResolvedValue([]);
    dialogMocks.confirm.mockResolvedValue(true);
    dialogMocks.open.mockResolvedValue(null);
    appSettings.save({
      ...defaultAppSettings,
      onboardingCompleted: true
    });
  });

  it('renders the browser-preview no-project shell and fallback title outside Tauri', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(false);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: null,
      workerStatus: 'error',
      lastRuntimeError:
        'Desktop commands require the Tauri host. The browser preview cannot run Rust or Python commands.'
    });

    render(Page);

    expect(await screen.findByText('Browser preview mode')).toBeTruthy();
    await waitFor(() => {
      expect(document.title).toBe('ScriptScore');
    });
    expect(desktopMocks.getExamWorkspaceState).not.toHaveBeenCalled();
  });

  it('loads Local Ollama model choices during first-run onboarding', async () => {
    appSettings.save({
      ...defaultAppSettings,
      onboardingCompleted: false,
      aiAssistEnabled: false
    });
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });

    render(Page);

    expect(await screen.findByText('First-run setup')).toBeTruthy();
    await waitFor(() => {
      expect(desktopMocks.listVisionModels).toHaveBeenCalledWith(
        'ollama_native',
        'http://127.0.0.1:11434',
        null
      );
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await fireEvent.click(await screen.findByRole('radio', { name: /Local Ollama/ }));
    await fireEvent.click(await screen.findByRole('combobox', { name: /Vision model:/ }));

    expect(await screen.findByRole('option', { name: 'llava:7b' })).toBeTruthy();
  });

  it('runs the Settings smoke test from the no-project shell without loading a project trace', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    });

    render(Page);

    expect(await screen.findByRole('button', { name: 'Create Project' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(await screen.findByRole('heading', { name: 'Connections' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Diagnostics' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Runtime smoke test' }));

    await waitFor(() => {
      expect(desktopMocks.runSmokePing).toHaveBeenCalledTimes(1);
    });

    expect(desktopMocks.getJobTrace).not.toHaveBeenCalled();
    expect(screen.queryByText('Smoke Test Results')).toBeNull();
    expect(await screen.findByText('1 step - 1 event')).toBeTruthy();
    expect(screen.queryByText('Action failed')).toBeNull();
  });

  it('docks no-project Settings notifications in the topbar', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    });

    render(Page);

    expect(await screen.findByRole('button', { name: 'Create Project' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(await screen.findByRole('heading', { name: 'Connections' })).toBeTruthy();

    notifications.pushError('Runtime smoke test failed', 0);
    await tick();

    const header = document.querySelector('header');
    const toast = screen.getByRole('alert');
    expect(header).toBeTruthy();
    expect(toast.textContent).toContain('Runtime smoke test failed');
    expect(toast.closest('header')).toBe(header);
  });

  it('does not render contextual setup or review trace controls', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Setup Log' })).toBeNull();

    await selectTemplateSetupSubstep('Review');

    expect(screen.queryByRole('button', { name: 'Analysis trace' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Rubric trace' })).toBeNull();
  });

  it('handles create-project completion even when the terminal event arrives before the job id resolves', async () => {
    const createJob = deferred<string>();
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.createProject.mockReturnValue(createJob.promise);
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByRole('button', { name: 'Create Project' })).toBeTruthy();
    await openCreateProjectForm();
    await populateCreateProjectForm();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create Project' })[1]);

    await waitFor(() => {
      expect(desktopMocks.createProject).toHaveBeenCalledTimes(1);
    });
    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        payload: hostWorkflowPayload('shell', {
          currentProject: projectSummary(),
          workerStatus: 'ready',
          lastRuntimeError: null
        })
      })
    );
    createJob.resolve('job_1');

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Create a Project' })).toBeNull();
  });

  it('passes the configured projects directory into project creation', async () => {
    appSettings.save({
      ...defaultAppSettings,
      onboardingCompleted: true,
      projectsDirectory: '/tmp/scriptscore-projects'
    });
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    });

    render(Page);

    expect(await screen.findByRole('button', { name: 'Create Project' })).toBeTruthy();
    await openCreateProjectForm();
    await populateCreateProjectForm();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create Project' })[1]);

    await waitFor(() => {
      expect(desktopMocks.createProject).toHaveBeenCalledWith(
        expect.objectContaining({ projectRoot: '/tmp/scriptscore-projects' }),
        expect.objectContaining({ projectsDirectory: '/tmp/scriptscore-projects' })
      );
    });
  });

  it('keeps the worker badge responsive during re-analyze and applies the completion refresh', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.reanalyzeQuestion.mockResolvedValue('job_analyze_1');
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.click(screen.getByRole('button', { name: 'Re Analyze' }));

    await waitFor(() => {
      expect(desktopMocks.reanalyzeQuestion).toHaveBeenCalledTimes(1);
    });
    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_progress',
        commandName: 'exam.analyze',
        workerStatus: 'busy',
        jobId: 'job_analyze_1',
        payload: {
          event: 'stage_progress',
          progress: {
            percent: 40
          }
        }
      })
    );

    expect(await screen.findByText('Busy 40%')).toBeTruthy();

    emitRuntimeEvent(
      runtimeJobEvent({
        commandName: 'exam.analyze',
        jobId: 'job_analyze_1',
        payload: {}
      })
    );

    await waitFor(() => {
      expect(desktopMocks.getExamWorkspaceState.mock.calls.length).toBeGreaterThanOrEqual(2);
    });
  });

  it('hides rubric generation after a rubric is approved', async () => {
    const state = analyzedWorkspaceState();
    state.questions[0]!.rubric = {
      status: 'approved',
      criteria: [],
      warnings: [],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_1'
    };

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(state);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');

    expect(screen.queryByRole('button', { name: 'Generate rubric' })).toBeNull();
    expect(desktopMocks.generateQuestionRubric).not.toHaveBeenCalled();
    expect(dialogMocks.confirm).not.toHaveBeenCalled();
  });

  it('preserves shell errors returned by create-project completion payloads', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    const failedShell: ShellState = {
      currentProject: projectSummary(),
      workerStatus: 'error',
      lastRuntimeError: 'worker unavailable',
      debugFeatures: { redactionToggle: false }
    };

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByRole('button', { name: 'Create Project' })).toBeTruthy();
    await openCreateProjectForm();
    await populateCreateProjectForm();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create Project' })[1]);

    await waitFor(() => {
      expect(desktopMocks.createProject).toHaveBeenCalledTimes(1);
    });
    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        payload: hostWorkflowPayload('shell', failedShell)
      })
    );

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await waitFor(() => {
      expect(get(shellState)).toEqual(failedShell);
    });
    expect(screen.getByText('worker unavailable')).toBeTruthy();
  });

  it('shows recovered stale desktop job notices as navbar toasts instead of workspace errors', async () => {
    const recoveryNotice = 'Recovered 2 stale desktop job records from a prior session.';
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: recoveryNotice,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await waitFor(() => {
      expect(get(notifications).some((toast) => toast.message === recoveryNotice)).toBe(true);
    });
    expect(screen.queryByRole('button', { name: 'Dismiss message' })).toBeNull();
    expect(screen.getByRole('status').textContent).toContain(recoveryNotice);
  });

  it('reloads workspace state when background analysis finishes', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState
      .mockResolvedValueOnce(workspaceState())
      .mockResolvedValueOnce(analyzedWorkspaceState());
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    expect(
      screen.getByText(
        'Analysis has not completed yet. Question text edits stay disabled until cleaned text is ready.'
      )
    ).toBeTruthy();

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        commandName: 'exam.analyze',
        jobId: 'job_analyze_1',
        payload: { ok: true }
      })
    );

    await waitFor(() => {
      expect(desktopMocks.getExamWorkspaceState).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(
        screen.queryByText(
          'Analysis has not completed yet. Question text edits stay disabled until cleaned text is ready.'
        )
      ).toBeNull();
    });
    expect(screen.getByDisplayValue('Cleaned question text.')).toBeTruthy();
  });

  it('exports the setup template through the native PDF save dialog', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    const exportedWorkspace = workspaceState({
      arucoStatus: {
        state: 'detected',
        totalMarkerCount: 4,
        pages: [{ pageNumber: 1, markerCount: 4, markerIds: [0, 1, 2, 3] }]
      }
    });
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(
      workspaceState({ arucoStatus: { state: 'not_detected', totalMarkerCount: 0, pages: [] } })
    );
    desktopMocks.exportStampedTemplatePdf.mockResolvedValue('job_export_template_1');
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });
    dialogMocks.save.mockResolvedValue('/tmp/midterm-1-template.pdf');

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'No Alignment Marks' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Add Alignment Stamps & Export PDF' }));

    await waitFor(() => {
      expect(dialogMocks.save).toHaveBeenCalledWith(
        expect.objectContaining({
          title: 'Export Template PDF',
          defaultPath: 'midterm-1-template.pdf',
          filters: [{ name: 'PDF', extensions: ['pdf'] }]
        })
      );
    });
    await waitFor(() => {
      expect(desktopMocks.exportStampedTemplatePdf).toHaveBeenCalledWith(
        '/tmp/midterm-1-template.pdf'
      );
    });
    expect(await screen.findByText('Creating Alignment Marks...')).toBeTruthy();

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;
    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_started',
        commandName: 'scans.pdf-stamp-aruco',
        workerStatus: 'busy',
        jobId: 'job_stamp_1',
        payload: {}
      })
    );
    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_finished',
        commandName: 'export_stamped_template_pdf',
        workerStatus: 'ready',
        jobId: 'job_export_template_1',
        payload: hostWorkflowPayload('workspace', exportedWorkspace)
      })
    );
    expect(await screen.findByRole('button', { name: 'Export template PDF' })).toBeTruthy();
    await waitFor(() => {
      expect(get(notifications).some((toast) => toast.message === 'Template PDF exported')).toBe(true);
    });
  });

  it('shows running exam analysis progress when entering Review after analysis starts', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    expect(get(workspaceView).activeTemplateSetupSubstep).toBe('setup');

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_started',
        commandName: 'exam.analyze',
        workerStatus: 'busy',
        jobId: 'job_analyze_running',
        payload: {}
      })
    );

    await selectTemplateSetupSubstep('Review');

    const questionOneButton = await screen.findByRole('button', {
      name: /Question 1/i
    });
    expect(within(questionOneButton).getByLabelText('Analyzing question')).toBeTruthy();
    expect(screen.getByText('Analyzing question...')).toBeTruthy();
  });

  it('shows the selected question as analyzing immediately after clicking Re Analyze', async () => {
    const reanalyzeJob = deferred<string>();
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(analyzedWorkspaceState());
    desktopMocks.reanalyzeQuestion.mockReturnValue(reanalyzeJob.promise);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    const questionOneButton = await screen.findByRole('button', {
      name: /Question 1/i
    });
    expect(within(questionOneButton).queryByLabelText('Analyzing question')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Re Analyze' }));

    await waitFor(() => {
      expect(desktopMocks.reanalyzeQuestion).toHaveBeenCalledWith(
        'question_1',
        expect.any(Object)
      );
    });
    expect(within(questionOneButton).getByLabelText('Analyzing question')).toBeTruthy();
    expect(screen.queryByText('Analyzing question...')).toBeNull();

    reanalyzeJob.resolve('job_analyze_1');
  });

  it('allows another question to be re-analyzed while one question is already running', async () => {
    const reanalyzeJob = deferred<string>();
    const initialWorkspace = multiQuestionWorkspaceState();
    initialWorkspace.questions = initialWorkspace.questions.map((question) => ({
      ...question,
      text: `Cleaned ${question.questionNumber}.`,
      analysis: {
        status: 'ok',
        questionTextClean: `Cleaned ${question.questionNumber}.`,
        questionContext: '',
        warnings: [],
        latestJobId: `job_analyze_${question.questionNumber}`
      }
    }));
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.reanalyzeQuestion.mockReturnValue(reanalyzeJob.promise);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');

    await fireEvent.click(screen.getByRole('button', { name: 'Re Analyze' }));
    await waitFor(() => {
      expect(desktopMocks.reanalyzeQuestion).toHaveBeenCalledWith(
        'question_1',
        expect.any(Object)
      );
    });

    await fireEvent.click(screen.getByRole('button', { name: /Question 2/i }));
    const secondQuestionReanalyze = screen.getByRole('button', { name: 'Re Analyze' });
    expect((secondQuestionReanalyze as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(secondQuestionReanalyze);

    await waitFor(() => {
      expect(desktopMocks.reanalyzeQuestion).toHaveBeenCalledTimes(2);
    });
    expect(desktopMocks.reanalyzeQuestion).toHaveBeenLastCalledWith(
      'question_2',
      expect.any(Object)
    );

    reanalyzeJob.resolve('job_analyze_1');
  });

  it('does not treat unscoped targeted re-analysis events as global Review progress', async () => {
    const reanalyzeJob = deferred<string>();
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    const initialWorkspace = multiQuestionWorkspaceState();
    initialWorkspace.questions[0] = {
      ...initialWorkspace.questions[0]!,
      text: 'Cleaned question text.',
      analysis: {
        status: 'ok',
        questionTextClean: 'Cleaned question text.',
        questionContext: '',
        warnings: [],
        latestJobId: 'job_analyze_1'
      }
    };
    initialWorkspace.questions[1] = {
      ...initialWorkspace.questions[1]!,
      analysis: {
        status: 'not_started',
        questionTextClean: null,
        questionContext: null,
        warnings: [],
        latestJobId: null
      }
    };
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.reanalyzeQuestion.mockReturnValue(reanalyzeJob.promise);
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    const questionOneButton = await screen.findByRole('button', {
      name: /Question 1/i
    });
    const questionTwoButton = await screen.findByRole('button', {
      name: /Question 2/i
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Re Analyze' }));
    await waitFor(() => {
      expect(desktopMocks.reanalyzeQuestion).toHaveBeenCalledWith(
        'question_1',
        expect.any(Object)
      );
    });
    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_progress',
        commandName: 'exam.analyze',
        workerStatus: 'busy',
        jobId: 'job_analyze_targeted_1',
        payload: {
          event: 'stage_progress',
          progress: { percent: 25 }
        }
      })
    );

    expect(within(questionOneButton).getByLabelText('Analyzing question')).toBeTruthy();
    expect(within(questionTwoButton).queryByLabelText('Analyzing question')).toBeNull();

    await fireEvent.click(questionTwoButton);
    expect(screen.queryByText('Analyzing question...')).toBeNull();
    expect(
      screen.getByText(
        'Analysis has not completed yet. Question text edits stay disabled until cleaned text is ready.'
      )
    ).toBeTruthy();

    reanalyzeJob.resolve('job_analyze_1');
  });

  it('updates the Review sidebar icon from the saved rubric response', async () => {
    const initialWorkspace = analyzedWorkspaceState();
    initialWorkspace.questions[0]!.rubric = {
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
    const savedWorkspace = structuredClone(initialWorkspace);
    savedWorkspace.questions[0]!.rubric = {
      ...initialWorkspace.questions[0]!.rubric!,
      status: 'approved',
      warnings: [{ code: 'rubric_points_mismatch', message: 'Criteria point mismatch.', scope: null }],
      approvedAt: '2026-04-08T00:00:00Z'
    };

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.saveRubricUpdate.mockResolvedValue(savedWorkspace);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    const questionOneButton = await screen.findByRole('button', {
      name: /Question 1/i
    });
    expect(within(questionOneButton).queryByLabelText('Rubric approved')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Approve rubric' }));

    await waitFor(() => {
      expect(desktopMocks.saveRubricUpdate).toHaveBeenCalledWith({
        questionId: 'question_1',
        criteria: expect.arrayContaining([expect.objectContaining({ criterionId: 'c1', points: 5 })]),
        approve: true
      });
    });
    expect(within(questionOneButton).getByLabelText('Rubric approved')).toBeTruthy();
    expect(screen.getByText('Approved rubric saved')).toBeTruthy();
    expect(get(notifications).some((toast) => toast.message === 'Approved rubric saved')).toBe(true);
  });

  it('keeps a warning icon after saving an empty rubric response', async () => {
    const initialWorkspace = analyzedWorkspaceState();
    initialWorkspace.questions[0]!.rubric = {
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
    const savedWorkspace = structuredClone(initialWorkspace);
    savedWorkspace.questions[0]!.rubric = {
      status: 'draft',
      criteria: [],
      warnings: [],
      approvedAt: null,
      latestJobId: 'job_rubric_1'
    };

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.saveRubricUpdate.mockResolvedValue(savedWorkspace);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    const questionOneButton = await screen.findByRole('button', {
      name: /Question 1/i
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Remove criterion' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Delete criterion' }));
    expect(within(questionOneButton).getByLabelText('Rubric warning')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(desktopMocks.saveRubricUpdate).toHaveBeenCalledWith({
        questionId: 'question_1',
        criteria: [],
        approve: false
      });
    });
    expect(within(questionOneButton).getByLabelText('Rubric warning')).toBeTruthy();
    expect(within(questionOneButton).queryByLabelText('Rubric generation queued')).toBeNull();
    await waitFor(() => {
      expect(screen.getByText('Review changes saved')).toBeTruthy();
    });
    expect(get(notifications).some((toast) => toast.message === 'Review changes saved')).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: '+ add criterion' }));
    expect(screen.getByText('Review changes saved')).toBeTruthy();
  });

  it('saves combined review changes as question edits before rubric edits with one toast', async () => {
    const initialWorkspace = analyzedWorkspaceState();
    const questionSavedWorkspace = structuredClone(initialWorkspace);
    questionSavedWorkspace.questions[0]!.text = 'Updated cleaned text.';
    const rubricSavedWorkspace = structuredClone(questionSavedWorkspace);
    rubricSavedWorkspace.questions[0]!.rubric = {
      status: 'draft',
      criteria: [
        {
          criterionId: '',
          label: '',
          points: 1,
          partialCreditGuidance: '',
          source: 'manual'
        }
      ],
      warnings: [],
      approvedAt: null,
      latestJobId: null
    };

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.saveQuestionEdits.mockResolvedValue(questionSavedWorkspace);
    desktopMocks.saveRubricUpdate.mockResolvedValue(rubricSavedWorkspace);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.input(screen.getByLabelText('Question text'), {
      target: { value: 'Updated cleaned text.' }
    });
    await fireEvent.click(screen.getByRole('button', { name: '+ add criterion' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(desktopMocks.saveQuestionEdits).toHaveBeenCalledTimes(1);
      expect(desktopMocks.saveRubricUpdate).toHaveBeenCalledTimes(1);
    });
    expect(desktopMocks.saveQuestionEdits.mock.invocationCallOrder[0]).toBeLessThan(
      desktopMocks.saveRubricUpdate.mock.invocationCallOrder[0]!
    );
    expect(desktopMocks.saveRubricUpdate).toHaveBeenCalledWith({
      questionId: 'question_1',
      criteria: expect.arrayContaining([expect.objectContaining({ points: 1 })]),
      approve: false
    });
    expect(get(notifications).filter((toast) => toast.message === 'Review changes saved')).toHaveLength(1);
    expect(desktopMocks.approveTemplateSetup).not.toHaveBeenCalled();
  });

  it('passes minor approved-edit impact through route Save', async () => {
    const initialWorkspace = analyzedWorkspaceState();
    initialWorkspace.questions[0]!.rubric = {
      status: 'approved',
      criteria: [
        {
          criterionId: 'criterion_1',
          label: 'Correctness',
          points: 5,
          partialCreditGuidance: 'Award up to 5 points.',
          source: 'manual'
        }
      ],
      warnings: [],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_1'
    };
    const savedWorkspace = structuredClone(initialWorkspace);
    savedWorkspace.questions[0]!.text = 'Updated cleaned text.';

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.saveQuestionEdits.mockResolvedValue(savedWorkspace);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.input(screen.getByLabelText('Question text'), {
      target: { value: 'Updated cleaned text.' }
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save as minor edit' }));

    await waitFor(() => {
      expect(desktopMocks.saveQuestionEdits).toHaveBeenCalledWith([
        expect.objectContaining({
          questionId: 'question_1',
          text: 'Updated cleaned text.',
          rubricEditImpact: 'minor'
        })
      ]);
    });
  });

  it('passes grading impact when rescinding approved rubric status without content edits', async () => {
    const initialWorkspace = analyzedWorkspaceState();
    initialWorkspace.questions[0]!.rubric = {
      status: 'approved',
      criteria: [
        {
          criterionId: 'criterion_1',
          label: 'Correctness',
          points: 5,
          partialCreditGuidance: 'Award up to 5 points.',
          source: 'manual'
        }
      ],
      warnings: [],
      approvedAt: '2026-04-08T00:00:00Z',
      latestJobId: 'job_rubric_1'
    };
    const savedWorkspace = structuredClone(initialWorkspace);
    savedWorkspace.questions[0]!.rubric = {
      ...initialWorkspace.questions[0]!.rubric!,
      status: 'draft',
      approvedAt: null
    };

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.saveRubricUpdate.mockResolvedValue(savedWorkspace);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.click(screen.getByRole('button', { name: 'Rescind approval' }));

    await waitFor(() => {
      expect(desktopMocks.saveRubricUpdate).toHaveBeenCalledWith({
        questionId: 'question_1',
        criteria: [
          {
            criterionId: 'criterion_1',
            label: 'Correctness',
            points: 5,
            partialCreditGuidance: 'Award up to 5 points.',
            source: 'manual'
          }
        ],
        approve: false,
        rubricEditImpact: 'grading'
      });
    });
  });

  it('does not enqueue automatic rubric jobs from the route when exam.analyze finishes on Setup', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    expect(get(workspaceView).activeTemplateSetupSubstep).toBe('setup');

    desktopMocks.ensureAutomaticRubricJobs.mockClear();

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        commandName: 'exam.analyze',
        jobId: 'job_analyze_route_1',
        payload: { ok: true }
      })
    );

    await flushScheduledAutomaticRubricEnsure();
    expect(desktopMocks.ensureAutomaticRubricJobs).not.toHaveBeenCalled();
  });

  it('enqueues automatic rubric jobs from the route when exam.analyze finishes on Exam Review', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    appSettings.save({
      ...defaultAppSettings,
      aiAssistEnabled: true,
      aiAssistCategories: {
        ...defaultAppSettings.aiAssistCategories,
        rubrics: true
      }
    });
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await flushScheduledAutomaticRubricEnsure();
    desktopMocks.ensureAutomaticRubricJobs.mockClear();

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent(
      runtimeJobEvent({
        commandName: 'exam.analyze',
        jobId: 'job_analyze_route_2',
        payload: { ok: true }
      })
    );

    await flushScheduledAutomaticRubricEnsure();
    expect(desktopMocks.ensureAutomaticRubricJobs).toHaveBeenCalledTimes(1);
  });

  it('updates the Students workflow stage chip when a Phase 06 runtime job starts', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    appSettings.save(configuredCanvasSettings());
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState
      .mockResolvedValueOnce(studentsWorkflowWorkspaceState('intake_ready'))
      .mockResolvedValueOnce(studentsWorkflowWorkspaceState('parse'));
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Students' }));
    expect(await screen.findByText('Exam Workflow')).toBeTruthy();
    await waitFor(() => {
      expect(screen.getAllByText('Jordan Rivera').length).toBeGreaterThan(0);
    });

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const handler = runtimeHandler as (event: RuntimeJobEvent) => void;

    handler(
      runtimeJobEvent({
        eventType: 'job_started',
        commandName: 'scans.parse',
        workerStatus: 'busy',
        jobId: 'job_parse_1',
        payload: {}
      })
    );

    await waitFor(() => {
      expect(screen.getAllByText('Parsing').length).toBeGreaterThan(0);
    });
  });

  it('preserves granular parse progress through the route refresh loop', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    appSettings.save(configuredCanvasSettings());
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState
      .mockResolvedValueOnce(studentsWorkflowWorkspaceState('intake_ready'))
      .mockResolvedValueOnce(studentsWorkflowWorkspaceState('parse'))
      .mockResolvedValueOnce(studentsWorkflowWorkspaceState('parse'));
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Students' }));
    expect(await screen.findByText('Exam Workflow')).toBeTruthy();
    await waitFor(() => {
      expect(screen.getAllByText('Jordan Rivera').length).toBeGreaterThan(0);
    });

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent = runtimeHandler as (event: RuntimeJobEvent) => void;
    const bar = () => screen.getByRole('progressbar', { name: 'Parsing' });

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_started',
        commandName: 'scans.parse',
        workerStatus: 'busy',
        jobId: 'job_parse_1',
        payload: {
          studentRef: 'student_1'
        }
      })
    );

    await waitFor(() => {
      expect(bar().getAttribute('aria-valuenow')).toBe('55');
    });
    await waitFor(() => {
      expect(desktopMocks.getExamWorkspaceState).toHaveBeenCalledTimes(2);
    });

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_progress',
        commandName: 'scans.parse',
        workerStatus: 'busy',
        jobId: 'job_parse_1',
        payload: {
          studentRef: 'student_1',
          event: 'started',
          data: { total_stages: 2 }
        }
      })
    );

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_progress',
        commandName: 'scans.parse',
        workerStatus: 'busy',
        jobId: 'job_parse_1',
        payload: {
          studentRef: 'student_1',
          event: 'stage_started',
          data: { stage_number: 2 }
        }
      })
    );

    await waitFor(() => {
      expect(bar().getAttribute('aria-valuenow')).toBe('63');
    });

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'workflow_state_updated',
        commandName: 'student.workflow',
        workerStatus: 'busy',
        jobId: 'job_workflow_1',
        payload: {
          workflowStateUpdated: true,
          studentRef: 'student_1',
          stage: 'parse',
          workflowStatus: 'running'
        }
      })
    );

    await waitFor(() => {
      expect(desktopMocks.getExamWorkspaceState).toHaveBeenCalledTimes(3);
    });
    await waitFor(() => {
      expect(bar().getAttribute('aria-valuenow')).toBe('63');
    });
  });

  it.each([
    {
      stage: 'alignment_review' as const,
      confirm: async () => {
        await fireEvent.click(screen.getByRole('button', { name: 'Confirm alignment → continue' }));
      },
      saveMock: () => desktopMocks.saveStudentAlignmentReview,
      confirmMock: () => desktopMocks.confirmStudentAlignment
    },
    {
      stage: 'detect_review' as const,
      confirm: async () => {
        await fireEvent.click(screen.getByRole('button', { name: 'Confirm Regions' }));
      },
      saveMock: () => desktopMocks.saveStudentDetectReview,
      confirmMock: () => desktopMocks.confirmStudentDetectReview
    },
    {
      stage: 'parse_review' as const,
      confirm: async () => {
        await fireEvent.input(screen.getByRole('textbox'), { target: { value: 'raw answer' } });
        await fireEvent.click(screen.getByRole('button', { name: 'Confirm answer → continue' }));
      },
      saveMock: () => desktopMocks.saveStudentParseReview,
      confirmMock: () => desktopMocks.confirmStudentParseReview
    }
  ])(
    'saves $stage review changes while a tracked student workflow host job is active',
    async ({ stage, confirm, saveMock, confirmMock }) => {
      let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
      appSettings.save(configuredCanvasSettings());
      desktopMocks.isDesktopHost.mockReturnValue(true);
      desktopMocks.getShellState.mockResolvedValue({
        currentProject: projectSummary(),
        workerStatus: 'ready',
        workerActivity: { activeJobs: [], pendingJobCount: 0 },
        lastRuntimeError: null,
        debugFeatures: { redactionToggle: false }
      });
      desktopMocks.getExamWorkspaceState.mockResolvedValue(studentWorkflowReviewWorkspaceState(stage));
      desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
        runtimeHandler = handler as (event: RuntimeJobEvent) => void;
        return vi.fn();
      });

      render(Page);

      expect(await screen.findByText('Student workflow review')).toBeTruthy();
      await fireEvent.click(screen.getByRole('button', { name: 'Students' }));
      expect(await screen.findByText('Exam Workflow')).toBeTruthy();
      await fireEvent.click(screen.getByRole('button', { name: 'Begin Workflow' }));
      await waitFor(() => {
        expect(desktopMocks.beginStudentWorkflow).toHaveBeenCalledTimes(1);
      });

      if (runtimeHandler === null) {
        throw new Error('runtime handler should be registered');
      }
      const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;
      emitRuntimeEvent(
        runtimeJobEvent({
          eventType: 'workflow_state_updated',
          commandName: 'student.workflow',
          workerStatus: 'ready',
          jobId: 'job_workflow_state_1',
          payload: {
            workflowStateUpdated: true,
            workflowStatus: 'ready'
          }
        })
      );
      expect(screen.getByRole('button', { name: 'Stop Workflow' })).toBeTruthy();

      await fireEvent.click(await screen.findByRole('button', { name: 'Review Jordan Rivera' }));
      await confirm();

      await waitFor(() => {
        expect(saveMock()).toHaveBeenCalledTimes(1);
      });
      expect(confirmMock()).not.toHaveBeenCalled();
    }
  );

  it('defers saved review continuation until active and pending desktop jobs drain', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    appSettings.save(configuredCanvasSettings());
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'busy',
      workerActivity: {
        activeJobs: [
          {
            jobId: 'job_active_1',
            commandName: 'grading.score-preliminary',
            startedAt: '2026-04-02T00:00:01Z'
          }
        ],
        pendingJobCount: 1
      },
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState
      .mockResolvedValueOnce(studentWorkflowReviewWorkspaceState('detect_review'))
      .mockResolvedValue(studentsWorkflowWorkspaceState('crop'));
    desktopMocks.saveStudentDetectReview.mockResolvedValue(studentsWorkflowWorkspaceState('crop'));
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Student workflow review')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Students' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Review Jordan Rivera' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm Regions' }));

    await waitFor(() => {
      expect(desktopMocks.saveStudentDetectReview).toHaveBeenCalledTimes(1);
    });
    expect(desktopMocks.beginStudentWorkflow).not.toHaveBeenCalled();

    if (runtimeHandler === null) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;
    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_finished',
        commandName: 'grading.score-preliminary',
        workerStatus: 'busy',
        jobId: 'job_active_1',
        payload: {
          schedulerActiveJobDetails: [],
          schedulerPendingJobs: 1
        }
      })
    );

    await tick();
    expect(desktopMocks.beginStudentWorkflow).not.toHaveBeenCalled();

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'job_finished',
        commandName: 'grading.draft-feedback',
        workerStatus: 'ready',
        jobId: 'job_pending_1',
        payload: {
          schedulerActiveJobDetails: [],
          schedulerPendingJobs: 0
        }
      })
    );

    await waitFor(() => {
      expect(desktopMocks.beginStudentWorkflow).toHaveBeenCalledTimes(1);
    });
  });

  it('renders the loading project shell while the workspace is still resolving', async () => {
    const workspaceLoad = deferred<ReturnType<typeof workspaceState>>();
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockReturnValue(workspaceLoad.promise);

    render(Page);

    expect(await screen.findByText('Loading project workspace...')).toBeTruthy();

    workspaceLoad.resolve(workspaceState());
    await screen.findByText('Question review in progress');
  });

  it('renders the project-loaded shell frame and updates the document title', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    expect(screen.queryByRole('menuitem', { name: 'Setup' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Settings' })).toBeTruthy();
    expect(screen.getByText('Instructor profile')).toBeTruthy();
    expect(screen.getByLabelText('Grading strictness')).toBeTruthy();
    await waitFor(() => {
      expect(document.title).toBe('ScriptScore Desktop - Midterm 1 · PHYS 221');
    });
  });

  it('loads Ollama vision models and runs the Settings smoke test without inline trace details', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.getJobTrace.mockResolvedValue({
      jobId: 'job_smoke_1',
      commandName: 'smoke.ping',
      state: 'finished',
      submittedAt: '2026-04-02T00:00:00Z',
      startedAt: '2026-04-02T00:00:01Z',
      finishedAt: '2026-04-02T00:00:02Z',
      request: {
        commandName: 'smoke.ping',
        payload: {
          ping: true
        }
      },
      result: {
        message: 'desktop runtime ready'
      },
      error: null,
      events: []
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));

    await waitFor(() => {
      expect(desktopMocks.listVisionModels).toHaveBeenCalledWith(
        'ollama_native',
        'http://127.0.0.1:11434',
        null
      );
    });
    expect(screen.queryByRole('button', { name: 'Refresh models' })).toBeNull();
    await fireEvent.click(screen.getByRole('combobox', { name: /Vision model:/ }));
    await fireEvent.click(await screen.findByRole('option', { name: 'llava:7b' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Diagnostics' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Runtime smoke test' }));

    await waitFor(() => {
      expect(desktopMocks.runSmokePing).toHaveBeenCalledTimes(1);
    });

    expect(desktopMocks.getJobTrace).not.toHaveBeenCalled();
    expect(await screen.findByText('1 step - 1 event')).toBeTruthy();
    expect(screen.queryByText('Smoke Test Results')).toBeNull();
  });

  it('opens Diagnostics trace history and loads selected trace details', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.listJobTraces.mockResolvedValue([
      {
        jobId: 'job_smoke_1',
        commandName: 'smoke.ping',
        state: 'succeeded',
        submittedAt: '2026-04-02T00:00:00Z',
        startedAt: '2026-04-02T00:00:01Z',
        finishedAt: '2026-04-02T00:00:02Z',
        eventCount: 1
      }
    ]);
    desktopMocks.getJobTrace.mockResolvedValue({
      jobId: 'job_smoke_1',
      commandName: 'smoke.ping',
      state: 'succeeded',
      submittedAt: '2026-04-02T00:00:00Z',
      startedAt: '2026-04-02T00:00:01Z',
      finishedAt: '2026-04-02T00:00:02Z',
      request: { commandName: 'smoke.ping' },
      result: { message: 'desktop runtime ready' },
      error: null,
      events: [
        {
          sequence: 1,
          eventType: 'started',
          progress: null,
          scope: null,
          data: { step: 'ping' },
          createdAt: '2026-04-02T00:00:01Z'
        }
      ]
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Diagnostics' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Trace history' }));

    await waitFor(() => {
      expect(desktopMocks.listJobTraces).toHaveBeenCalledTimes(1);
      expect(desktopMocks.getJobTrace).toHaveBeenCalledWith('job_smoke_1', null);
    });

    await fireEvent.click(await screen.findByRole('tab', { name: 'Request' }));
    expect(await screen.findByText(/"commandName": "smoke\.ping"/)).toBeTruthy();
  });

  it('opens trace history from the worker popover filtered to active jobs', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'busy',
      workerActivity: {
        activeJobs: [
          {
            jobId: 'job_active_1',
            commandName: 'grading.score-preliminary',
            startedAt: '2026-04-02T00:00:01Z'
          }
        ],
        pendingJobCount: 2
      },
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.listJobTraces.mockResolvedValue([
      {
        jobId: 'job_active_1',
        commandName: 'grading.score-preliminary',
        state: 'running',
        submittedAt: '2026-04-02T00:00:00Z',
        startedAt: '2026-04-02T00:00:01Z',
        finishedAt: null,
        eventCount: 1,
        studentRefs: ['student_1']
      },
      {
        jobId: 'job_done_1',
        commandName: 'exam.setup',
        state: 'succeeded',
        submittedAt: '2026-04-01T00:00:00Z',
        startedAt: '2026-04-01T00:00:01Z',
        finishedAt: '2026-04-01T00:00:02Z',
        eventCount: 1,
        studentRefs: []
      }
    ]);
    desktopMocks.getJobTrace.mockResolvedValue({
      jobId: 'job_active_1',
      commandName: 'grading.score-preliminary',
      state: 'running',
      submittedAt: '2026-04-02T00:00:00Z',
      startedAt: '2026-04-02T00:00:01Z',
      finishedAt: null,
      studentRefs: ['student_1'],
      request: { studentRef: 'student_1' },
      result: null,
      error: null,
      events: []
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'busy' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Show More' }));

    await waitFor(() => {
      expect(desktopMocks.listJobTraces).toHaveBeenCalledTimes(1);
      expect(desktopMocks.getJobTrace).toHaveBeenCalledWith('job_active_1', null);
    });
    expect(screen.getByRole('dialog', { name: 'Trace history' })).toBeTruthy();
    expect(screen.getByText('grading.score-preliminary')).toBeTruthy();
    expect(screen.queryByText('exam.setup')).toBeNull();
  });

  it('opens unfiltered trace history from the worker popover when no job is active', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      workerActivity: {
        activeJobs: [],
        pendingJobCount: 0
      },
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.listJobTraces.mockResolvedValue([
      {
        jobId: 'job_done_1',
        commandName: 'exam.setup',
        state: 'succeeded',
        submittedAt: '2026-04-01T00:00:00Z',
        startedAt: '2026-04-01T00:00:01Z',
        finishedAt: '2026-04-01T00:00:02Z',
        eventCount: 1,
        studentRefs: []
      }
    ]);
    desktopMocks.getJobTrace.mockResolvedValue({
      jobId: 'job_done_1',
      commandName: 'exam.setup',
      state: 'succeeded',
      submittedAt: '2026-04-01T00:00:00Z',
      startedAt: '2026-04-01T00:00:01Z',
      finishedAt: '2026-04-01T00:00:02Z',
      studentRefs: [],
      request: { commandName: 'exam.setup' },
      result: { ok: true },
      error: null,
      events: []
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'ready' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Show More' }));

    await waitFor(() => {
      expect(desktopMocks.listJobTraces).toHaveBeenCalledTimes(1);
      expect(desktopMocks.getJobTrace).toHaveBeenCalledWith('job_done_1', null);
    });
    expect(screen.getByRole('dialog', { name: 'Trace history' })).toBeTruthy();
    expect(screen.getByText('exam.setup')).toBeTruthy();
    expect(screen.queryByText('Active jobs:')).toBeNull();
  });

  it('persists Settings edits immediately without a Save settings action', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(screen.queryByRole('button', { name: /Save settings/i })).toBeNull();
    await fireEvent.click(await screen.findByRole('button', { name: 'Preferences' }));
    expect(await screen.findByText('Dark theme')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: /Dark theme/i }));

    await waitFor(() => {
      expect(get(appSettings).theme).toBe('light');
    });
  });

  it('applies Settings edits immediately without forcing a project setup sync', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(await screen.findByRole('heading', { name: 'Connections' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('combobox', { name: /Vision model:/ }));
    await fireEvent.click(screen.getByRole('option', { name: 'llava:7b' }));

    expect(desktopMocks.saveProjectConfig).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(get(appSettings).llmModel).toBe('llava:7b');
    });
  });

  it('keeps immediate Settings edits when choosing a projects folder', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    dialogMocks.open.mockResolvedValue('/tmp/new-scriptscore-projects');

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(await screen.findByRole('heading', { name: 'Connections' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('combobox', { name: /Vision model:/ }));
    await fireEvent.click(screen.getByRole('option', { name: 'llava:7b' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Storage' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Choose folder' })[0]);

    await waitFor(() => {
      expect(get(appSettings).llmModel).toBe('llava:7b');
      expect(get(appSettings).projectsDirectory).toBe('/tmp/new-scriptscore-projects');
    });
  });

  it('chooses and clears the PII Paddle model directory from storage settings', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    dialogMocks.open.mockResolvedValue('/tmp/paddle-models');

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Storage' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Choose folder' })[1]);

    await waitFor(() => {
      expect(get(appSettings).piiPaddleModelDir).toBe('/tmp/paddle-models');
    });
    expect(dialogMocks.open).toHaveBeenLastCalledWith(
      expect.objectContaining({
        directory: true,
        multiple: false,
        title: 'Choose PII Paddle Model Folder'
      })
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Clear model folder' }));
    expect(get(appSettings).piiPaddleModelDir).toBeNull();
  });

  it('saves the setup display name and refreshes the shell title and recent-project entry', async () => {
    const savedWorkspace = workspaceState({
      project: {
        ...projectSummary(),
        displayName: 'Chemistry Final',
        courseCode: 'CHEM 301'
      },
      projectConfig: {
        ...workspaceState().projectConfig!,
        displayName: 'Chemistry Final',
        courseCode: 'CHEM 301'
      }
    });
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.saveProjectConfig.mockResolvedValue(savedWorkspace);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.input(screen.getByPlaceholderText('Midterm 1'), {
      target: { value: 'Chemistry Final' }
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(desktopMocks.saveProjectConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          displayName: 'Chemistry Final'
        }),
        expect.any(Object)
      );
    });
    await waitFor(() => {
      expect(document.title).toBe('ScriptScore Desktop - Chemistry Final · CHEM 301');
    });
    expect(await screen.findByText('Template setup saved')).toBeTruthy();
    expect(get(notifications).some((toast) => toast.message === 'Template setup saved')).toBe(true);

    await fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(screen.getAllByText('Template setup saved').length).toBeGreaterThan(0);

    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(screen.getAllByText('Template setup saved').length).toBeGreaterThan(0);
    });
    await fireEvent.input(screen.getByLabelText('Subject'), {
      target: { value: 'Chemistry' }
    });
    expect(screen.getAllByText('Template setup saved').length).toBeGreaterThan(0);

    const recentProjects = JSON.parse(
      globalThis.localStorage.getItem('scriptscore-recent-projects') ?? '[]'
    ) as Array<{ displayName: string; courseCode: string | null }>;
    expect(recentProjects[0]).toMatchObject({
      displayName: 'Chemistry Final',
      courseCode: 'CHEM 301'
    });
  });

  it('builds re-analyze requests from the saved app settings instead of project setup', async () => {
    globalThis.localStorage.setItem(
      'scriptscore-app-settings',
      JSON.stringify({
        llmProvider: 'ollama_native',
        llmBaseUrl: 'http://127.0.0.1:11434',
        llmModel: 'llava:7b',
        llmApiKey: null,
        instructorProfile: {
          gradingStrictness: 'balanced',
          syntaxLeniency: 'medium',
          ocrTolerance: 'medium',
          partialCreditStyle: 'balanced',
          feedbackStyle: 'brief',
          enabledTags: {
            gradingStrictness: true,
            syntaxLeniency: false,
            ocrTolerance: false,
            partialCreditStyle: false,
            feedbackStyle: true
          },
          additionalGuidance: ''
        },
        aiAssistEnabled: true,
        theme: 'dark'
      })
    );
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());
    desktopMocks.reanalyzeQuestion.mockResolvedValue('job_analyze_1');

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.click(screen.getByRole('button', { name: 'Re Analyze' }));

    await waitFor(() => {
      expect(desktopMocks.reanalyzeQuestion).toHaveBeenCalledWith(
        'question_1',
        expect.objectContaining({
          llmModel: 'llava:7b',
          llmProvider: 'ollama_native',
          aiAssistEnabled: true
        })
      );
    });
  });

  it('blocks re-analyze while question edits are unsaved', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(analyzedWorkspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.input(screen.getByDisplayValue('Cleaned question text.'), {
      target: { value: 'Edited locally' }
    });

    const reanalyzeButton = screen.getByRole('button', { name: 'Re Analyze' });
    expect((reanalyzeButton as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(reanalyzeButton);

    expect(desktopMocks.reanalyzeQuestion).not.toHaveBeenCalled();
  });

  it('keeps later rail destinations clickable and renders the results workspace', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Students' }));
    // Students step uses StudentWorkflowWorkspace: default pane is workflow home (not the old intake drop heading).
    expect(await screen.findByText('Exam Workflow')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Upload Submission' })).toBeTruthy();
    expect(screen.getByText(/0 submissions ready/)).toBeTruthy();
    // Intake processor + prerequisite copy only appear after entering intake mode (e.g. upload); prerequisites are still false here.
    expect(screen.queryByRole('heading', { name: 'Intake processor' })).toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Setup' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation' }));
    expect(await screen.findByText('No moderation-ready answers yet')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Results' }));
    expect(
      await screen.findByRole('heading', {
        name: 'Select a student'
      })
    ).toBeTruthy();
    expect(screen.getByText('No graded submissions are ready for Results yet.')).toBeTruthy();
  });

  it('refreshes moderation display names when the shared roster cache becomes ready later', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    let rosterSnapshot: import('$lib/types').LmsRosterCacheSnapshot = {
      status: 'loading' as const,
      projectPath: '/tmp/midterm-1',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: [] as Array<{ userId: string; displayName: string; sortKey: string }>,
      lastError: null,
      idleReason: null
    };

    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: {
        ...projectSummary(),
        lmsCourseId: 'persisted-course-id'
      },
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(
      workspaceState({
        status: 'approved',
        statusLabel: 'Ready for moderation',
        project: {
          ...projectSummary(),
          lmsCourseId: 'persisted-course-id'
        },
        projectConfig: {
          ...workspaceState().projectConfig!,
          lmsCourseId: 'persisted-course-id'
        },
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_canvas_1' }],
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
              pageArtifacts: [],
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
                  verifiedText: 'verified answer text',
                  reviewRequired: false,
                  verified: true,
                  stale: false,
                  gradingStatus: 'draft_ready',
                  gradingConfidence: 'high',
                  gradingConfidenceReason: null,
                  questionMaxPoints: 5,
                  totalPointsAwarded: 4,
                  feedbackText: 'Strong overall response.',
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
      })
    );
    desktopMocks.getLmsRosterCacheState.mockImplementation(async () => rosterSnapshot);
    desktopMocks.ensureLmsRosterPreload.mockImplementation(async () => rosterSnapshot);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_canvas_1');
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Moderation' }));
    expect(await screen.findByText((_, element) => element?.textContent === 'Q1 - Explain westward expansion.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Moderation view settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Actual student names/i }));

    expect(screen.getByText('student_1')).toBeTruthy();
    expect(screen.queryByText('Ada Lovelace')).toBeNull();

    rosterSnapshot = {
      ...rosterSnapshot,
      status: 'ready',
      rows: [{ userId: 'canvas_1', displayName: 'Ada Lovelace', sortKey: 'lovelace, ada' }]
    };

    if (!runtimeHandler) {
      throw new Error('runtime handler should be registered');
    }

    const emitRuntimeEvent: (event: RuntimeJobEvent) => void = runtimeHandler;

    emitRuntimeEvent({
      eventType: 'lms_roster_cache_updated',
      commandName: 'lms.roster',
      workerStatus: 'ready',
      requestId: null,
      jobId: null,
      payload: {}
    });

    await waitFor(() => {
      expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    });
  });

  it('shows incremental Results upload progress from runtime events', async () => {
    const uploadRequest = deferred<import('$lib/types').ResultsLmsUploadResponse>();
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;

    appSettings.save(configuredCanvasSettings());
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: {
        ...projectSummary(),
        lmsCourseId: 'course_1'
      },
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(resultsUploadWorkspaceState());
    desktopMocks.runResultsLmsUpload.mockReturnValue(uploadRequest.promise);
    desktopMocks.listenRuntimeJobEvents.mockImplementation(async (handler) => {
      runtimeHandler = handler as (event: RuntimeJobEvent) => void;
      return vi.fn();
    });

    render(Page);

    expect(await screen.findByRole('button', { name: 'Results' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Results' }));
    expect(await screen.findByRole('heading', { name: 'student_1' })).toBeTruthy();

    await fireEvent.click(screen.getByLabelText('Select all filtered students'));
    await fireEvent.click(screen.getByRole('button', { name: 'Upload' }));
    await waitFor(() => {
      expect(desktopMocks.runResultsLmsUpload).toHaveBeenCalledWith({
        mode: 'live',
        studentRefs: ['student_1']
      });
    });

    if (!runtimeHandler) {
      throw new Error('runtime handler should be registered');
    }
    const emitRuntimeEvent = runtimeHandler as (event: RuntimeJobEvent) => void;

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'results_lms_upload_batch_started',
        commandName: 'results.lms-upload',
        workerStatus: 'busy',
        payload: {
          batchId: 'batch_1',
          mode: 'live',
          studentRefs: ['student_1']
        }
      })
    );
    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'results_lms_upload_student_started',
        commandName: 'results.lms-upload',
        workerStatus: 'busy',
        payload: {
          batchId: 'batch_1',
          studentRef: 'student_1'
        }
      })
    );

    await waitFor(() => {
      expect(screen.getAllByText('Uploading').length).toBeGreaterThan(0);
    });

    emitRuntimeEvent(
      runtimeJobEvent({
        eventType: 'results_lms_upload_student_finished',
        commandName: 'results.lms-upload',
        workerStatus: 'busy',
        payload: {
          batchId: 'batch_1',
          studentRef: 'student_1',
          status: 'uploaded'
        }
      })
    );

    await waitFor(() => {
      expect(screen.getAllByText('Uploaded').length).toBeGreaterThan(0);
    });

    uploadRequest.resolve({
      attempt: {
        attemptId: 'attempt_1',
        mode: 'live',
        provider: 'canvas',
        courseId: 'course_1',
        assignmentId: 'assignment_1',
        startedAt: '1',
        finishedAt: '2',
        attemptedCount: 1,
        successCount: 1,
        failureCount: 0,
        studentResults: []
      },
      workspace: (() => {
        const uploadedWorkspace = resultsUploadWorkspaceState();
        return {
          ...uploadedWorkspace,
          resultsLmsRows: uploadedWorkspace.resultsLmsRows!.map((row) => ({
            ...row,
            uploaded: true,
            lastUploadAttemptId: 'attempt_1'
          })),
          resultsLmsState: {
            selectedTarget: uploadedWorkspace.resultsLmsState!.selectedTarget,
            finalizationRecords: uploadedWorkspace.resultsLmsState!.finalizationRecords,
            uploadAttempts: [
              {
                attemptId: 'attempt_1',
                mode: 'live',
                provider: 'canvas',
                courseId: 'course_1',
                assignmentId: 'assignment_1',
                startedAt: '1',
                finishedAt: '2',
                attemptedCount: 1,
                successCount: 1,
                failureCount: 0,
                studentResults: []
              }
            ]
          }
        };
      })()
    });
  });

  it('does not invoke Results export when the save dialog is cancelled', async () => {
    appSettings.save(configuredCanvasSettings());
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: {
        ...projectSummary(),
        lmsCourseId: 'course_1'
      },
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(resultsUploadWorkspaceState());
    dialogMocks.save.mockResolvedValue(null);

    render(Page);

    expect(await screen.findByRole('button', { name: 'Results' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Results' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Student display settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Show Export action/i }));
    await fireEvent.click(
      within(screen.getByRole('region', { name: 'Student results list' })).getAllByRole(
        'checkbox'
      )[1]
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Export' }));
    await fireEvent.click(screen.getByRole('button', { name: 'CSV' }));

    await waitFor(() => expect(dialogMocks.save).toHaveBeenCalled());
    expect(desktopMocks.runResultsExport).not.toHaveBeenCalled();
  });

  it('passes HTML Results export choice with the selected destination', async () => {
    appSettings.save(configuredCanvasSettings());
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: {
        ...projectSummary(),
        lmsCourseId: 'course_1'
      },
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(resultsUploadWorkspaceState());
    desktopMocks.runResultsExport.mockResolvedValue({
      format: 'html_zip',
      destinationPath: '/tmp/results.zip',
      exportedCount: 1
    });
    dialogMocks.save.mockResolvedValueOnce('/tmp/results.zip');

    render(Page);

    expect(await screen.findByRole('button', { name: 'Results' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Results' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Student display settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Show Export action/i }));
    await fireEvent.click(screen.getByLabelText('Select all filtered students'));

    await fireEvent.click(screen.getByRole('button', { name: 'Export' }));
    await fireEvent.click(screen.getByRole('button', { name: 'HTML' }));
    await waitFor(() =>
      expect(desktopMocks.runResultsExport).toHaveBeenCalledWith({
        format: 'html_zip',
        studentRefs: ['student_1'],
        destinationPath: '/tmp/results.zip'
      })
    );

  });

  it('passes CSV Results export choice with the selected destination', async () => {
    appSettings.save(configuredCanvasSettings());
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: {
        ...projectSummary(),
        lmsCourseId: 'course_1'
      },
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(resultsUploadWorkspaceState());
    desktopMocks.runResultsExport.mockResolvedValue({
      format: 'csv',
      destinationPath: '/tmp/results.csv',
      exportedCount: 1
    });
    dialogMocks.save.mockResolvedValueOnce('/tmp/results.csv');

    render(Page);

    expect(await screen.findByRole('button', { name: 'Results' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Results' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Student display settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Show Export action/i }));
    await fireEvent.click(screen.getByLabelText('Select all filtered students'));

    await fireEvent.click(screen.getByRole('button', { name: 'Export' }));
    await fireEvent.click(screen.getByRole('button', { name: 'CSV' }));
    await waitFor(() =>
      expect(desktopMocks.runResultsExport).toHaveBeenCalledWith({
        format: 'csv',
        studentRefs: ['student_1'],
        destinationPath: '/tmp/results.csv'
      })
    );
  });

  it('hides the redactions-required toggle in the normal setup flow', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    const continueButton = screen.getByRole('button', { name: 'Continue' });
    expect(screen.queryByRole('button', { name: 'Redactions Required' })).toBeNull();
    expect((continueButton as HTMLButtonElement).disabled).toBe(true);
  });

  it('supports the debug redactions-required toggle and moves into review when disabled', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: true }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    const skipToggle = screen.getByRole('button', { name: 'Redactions Required' });
    const continueButton = screen.getByRole('button', { name: 'Continue' });
    expect((continueButton as HTMLButtonElement).disabled).toBe(true);
    expect(skipToggle.getAttribute('aria-pressed')).toBe('true');

    await fireEvent.click(skipToggle);

    expect(skipToggle.getAttribute('aria-pressed')).toBe('false');
    expect((continueButton as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(continueButton);

    expect(await screen.findByText('Question 1 of 1 (Page 1)')).toBeTruthy();
  });

  it('disables the redactions-required toggle once a region exists', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: true }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(multiQuestionWorkspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    const continueButton = screen.getByRole('button', { name: 'Continue' });
    const skipToggle = screen.getByRole('button', { name: 'Redactions Required' });

    expect((continueButton as HTMLButtonElement).disabled).toBe(false);
    expect((skipToggle as HTMLButtonElement).disabled).toBe(true);
    expect(skipToggle.getAttribute('aria-pressed')).toBe('true');
  });

  it('loads Canvas courses in setup and selects a course from the searchable popup', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    appSettings.save(configuredCanvasSettings());
    desktopMocks.listCanvasCourses.mockResolvedValue([
      { lmsCourseId: 'course_1', name: 'Biology 101', courseCode: 'BIO 101' },
      { lmsCourseId: 'course_2', name: 'Chemistry Lab', courseCode: 'CHEM 202' }
    ]);
    desktopMocks.listLmsAssignmentsForCourse.mockResolvedValue([
      { assignmentId: 'assignment_1', name: 'Lab Report', pointsPossible: 20 },
      { assignmentId: 'assignment_2', name: 'Final Exam', pointsPossible: 100 }
    ]);
    const savedProjectConfigWorkspace = workspaceState({
      projectConfig: {
        ...workspaceState().projectConfig!,
        lmsCourseId: 'course_2',
        courseCode: 'CHEM 202',
        lmsAssignmentId: null
      }
    });
    const savedAssignmentWorkspace = workspaceState({
      projectConfig: {
        ...workspaceState().projectConfig!,
        lmsCourseId: 'course_2',
        courseCode: 'CHEM 202',
        lmsAssignmentId: 'assignment_2'
      },
      resultsLmsState: {
        selectedTarget: {
          provider: 'canvas',
          courseId: 'course_2',
          assignmentId: 'assignment_2'
        },
        finalizationRecords: [],
        uploadAttempts: []
      }
    });
    desktopMocks.saveProjectConfig.mockResolvedValue(savedProjectConfigWorkspace);
    desktopMocks.saveResultsLmsAssignment.mockResolvedValue(savedAssignmentWorkspace);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Setup');
    await waitFor(() => {
      expect(desktopMocks.listCanvasCourses).toHaveBeenCalledWith(
        'https://canvas.example.test',
        'token'
      );
    });

    await fireEvent.click(screen.getByRole('combobox', { name: 'Course' }));
    await fireEvent.input(screen.getByPlaceholderText('Search courses'), {
      target: { value: 'chem' }
    });
    expect(screen.queryByText('Biology 101')).toBeNull();
    await fireEvent.click(screen.getByRole('option', { name: /Chemistry Lab/ }));
    await waitFor(() => {
      expect(desktopMocks.listLmsAssignmentsForCourse).toHaveBeenCalledWith('course_2');
    });
    expect(
      screen.getByLabelText(
        'You cannot upload results to the LMS without first selecting an assignment.'
      )
    ).toBeTruthy();
    await fireEvent.click(screen.getByRole('combobox', { name: 'Assignment' }));
    await fireEvent.input(screen.getByPlaceholderText('Search assignments'), {
      target: { value: 'final' }
    });
    expect(screen.queryByText('Lab Report')).toBeNull();
    await fireEvent.click(screen.getByRole('option', { name: /Final Exam/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(desktopMocks.saveProjectConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          lmsCourseId: 'course_2',
          courseCode: 'CHEM 202',
          lmsAssignmentId: null
        }),
        expect.any(Object)
      );
    });
    await waitFor(() => {
      expect(desktopMocks.saveResultsLmsAssignment).toHaveBeenCalledWith({
        assignmentId: 'assignment_2'
      });
    });
    expect(await screen.findByText('Template setup saved')).toBeTruthy();
  });

  it('ignores stale assignment loads after changing LMS courses', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    appSettings.save(configuredCanvasSettings());
    const firstAssignments =
      deferred<Array<{ assignmentId: string; name: string; pointsPossible: number }>>();
    const secondAssignments =
      deferred<Array<{ assignmentId: string; name: string; pointsPossible: number }>>();
    desktopMocks.listCanvasCourses.mockResolvedValue([
      { lmsCourseId: 'course_1', name: 'Biology 101', courseCode: 'BIO 101' },
      { lmsCourseId: 'course_2', name: 'Chemistry Lab', courseCode: 'CHEM 202' }
    ]);
    desktopMocks.listLmsAssignmentsForCourse.mockImplementation((courseId: string) => {
      if (courseId === 'course_1') return firstAssignments.promise;
      if (courseId === 'course_2') return secondAssignments.promise;
      return Promise.resolve([]);
    });
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(workspaceState());

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Setup');
    await fireEvent.click(await screen.findByRole('combobox', { name: 'Course' }));
    await fireEvent.click(screen.getByRole('option', { name: /Biology 101/ }));
    await waitFor(() => {
      expect(desktopMocks.listLmsAssignmentsForCourse).toHaveBeenCalledWith('course_1');
    });

    await fireEvent.click(screen.getByRole('combobox', { name: 'Course' }));
    await fireEvent.click(screen.getByRole('option', { name: /Chemistry Lab/ }));
    await waitFor(() => {
      expect(desktopMocks.listLmsAssignmentsForCourse).toHaveBeenCalledWith('course_2');
    });

    secondAssignments.resolve([{ assignmentId: 'assignment_2', name: 'Chemistry Final', pointsPossible: 100 }]);
    await waitFor(() => {
      expect(screen.getByRole('combobox', { name: 'Assignment' })).toBeTruthy();
    });
    firstAssignments.resolve([{ assignmentId: 'assignment_1', name: 'Biology Homework', pointsPossible: 10 }]);
    await tick();

    await fireEvent.click(screen.getByRole('combobox', { name: 'Assignment' }));
    expect(screen.getByRole('option', { name: /Chemistry Final/ })).toBeTruthy();
    expect(screen.queryByText('Biology Homework')).toBeNull();
  });

  it('clears the LMS course and hides local course metadata fields', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    appSettings.save(configuredCanvasSettings());
    desktopMocks.listCanvasCourses.mockResolvedValue([
      { lmsCourseId: 'course_1', name: 'Biology 101', courseCode: 'BIO 101' }
    ]);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(
      workspaceState({
        project: {
          ...projectSummary(),
          courseCode: 'BIO 101',
          lmsCourseId: 'course_1'
        },
        projectConfig: {
          ...workspaceState().projectConfig!,
          courseCode: 'BIO 101',
          lmsCourseId: 'course_1'
        }
      })
    );

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Setup');
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Course' })).toBeTruthy());
    await fireEvent.click(screen.getByRole('combobox', { name: 'Course' }));
    await fireEvent.click(screen.getByRole('option', { name: 'No LMS course' }));

    expect(screen.queryByPlaceholderText('HIST 201')).toBeNull();
    expect(screen.queryByLabelText('Assignment')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(desktopMocks.saveProjectConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          lmsCourseId: null,
          courseCode: 'BIO 101'
        }),
        expect.any(Object)
      );
    });
  });

  it('renders persisted workspace warnings in the shared message area', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(
      workspaceState({
        warnings: [
          {
            code: 'template_parse_warning',
            message: 'One template label could not be matched automatically.',
            scope: '{"page":1}'
          }
        ]
      })
    );

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    expect(screen.getByText('One template label could not be matched automatically.')).toBeTruthy();
  });

  it('preserves the selected question across a saved workspace reload', async () => {
    const initialWorkspace = multiQuestionWorkspaceState();
    const savedWorkspace = multiQuestionWorkspaceState();
    savedWorkspace.questions = savedWorkspace.questions.map((question: ExamWorkspaceState['questions'][number]) =>
      question.questionId === 'question_2'
        ? { ...question, text: 'Explain sp2 hybridization.', maxPoints: 9 }
        : question
    );
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(initialWorkspace);
    desktopMocks.saveQuestionEdits.mockResolvedValue(savedWorkspace);

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.click(screen.getByRole('button', { name: /Question 2/i }));

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    await fireEvent.input(textarea, {
      target: { value: 'Explain sp2 hybridization.' }
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(desktopMocks.saveQuestionEdits).toHaveBeenCalledWith([
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Explain westward expansion.',
        questionContext: ''
      },
      {
        questionId: 'question_2',
        questionNumber: 2,
        pageNumber: 2,
        maxPoints: 8,
        text: 'Explain sp2 hybridization.',
        questionContext: ''
      }
    ]);
    await waitFor(() => {
      expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe(
        'Explain sp2 hybridization.'
      );
    });
    await waitFor(() => {
      expect(screen.getByText('Review changes saved')).toBeTruthy();
    });
    expect(get(notifications).some((toast) => toast.message === 'Review changes saved')).toBe(true);
    await fireEvent.input(screen.getByRole('textbox'), {
      target: { value: 'Explain sp3 hybridization.' }
    });
    expect(screen.getByText('Review changes saved')).toBeTruthy();
    expect(screen.getByText('Question 2 of 2 (Page 2)')).toBeTruthy();
  });

  it('restores persisted question state after remount instead of keeping unsaved drafts', async () => {
    const persistedWorkspace = workspaceState();
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(persistedWorkspace);

    const firstRender = render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    await fireEvent.input(textarea, {
      target: { value: 'Unsaved local draft text.' }
    });
    expect(textarea.value).toBe('Unsaved local draft text.');

    firstRender.unmount();
    shellState.set({
      currentProject: null,
      workerStatus: 'starting',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    workspaceView.reset();

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await waitFor(() => {
      expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe(
        'Explain westward expansion.'
      );
    });
  });

  it('resets frontend-only selection state after close and reopen', async () => {
    const persistedWorkspace = multiQuestionWorkspaceState();
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(persistedWorkspace);
    dialogMocks.open.mockResolvedValue('/tmp/midterm-1');

    render(Page);

    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await fireEvent.click(screen.getByRole('button', { name: /Question 2/i }));
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe(
      'Explain orbital hybridization.'
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Close Project' }));
    expect(await screen.findByText('Create Project')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Open Project' }));
    expect(await screen.findByText('Question review in progress')).toBeTruthy();
    await selectTemplateSetupSubstep('Review');
    await waitFor(() => {
      expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe(
        'Explain westward expansion.'
      );
    });
  });

  it('renders persisted template-setup failures after workspace rehydrate', async () => {
    desktopMocks.isDesktopHost.mockReturnValue(true);
    desktopMocks.getShellState.mockResolvedValue({
      currentProject: projectSummary(),
      workerStatus: 'ready',
      lastRuntimeError: null
    });
    desktopMocks.getExamWorkspaceState.mockResolvedValue(
      workspaceState({
        status: 'failed',
        statusLabel: 'Template setup failed',
        workflowStage: 'template_setup_failed',
        workflowLabel: 'Template setup failed',
        failureMessage: 'The template worker could not be started.',
        questions: [],
        redactionRegions: [],
        canApprove: false,
        canApproveRubric: false
      })
    );

    render(Page);

    expect(await screen.findByText('Template setup failed')).toBeTruthy();
    expect(screen.getByText('The template worker could not be started.')).toBeTruthy();
  });
});
