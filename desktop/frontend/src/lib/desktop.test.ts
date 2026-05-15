// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { appSettings, defaultAppSettings } from '$lib/stores/appSettings';

const coreMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
  convertFileSrc: vi.fn((path: string) => `asset://${path}`)
}));

const eventMocks = vi.hoisted(() => ({
  listen: vi.fn()
}));

const appMocks = vi.hoisted(() => ({
  getVersion: vi.fn()
}));

const shellMocks = vi.hoisted(() => ({
  open: vi.fn()
}));

vi.mock('@tauri-apps/api/core', () => coreMocks);
vi.mock('@tauri-apps/api/event', () => eventMocks);
vi.mock('@tauri-apps/api/app', () => appMocks);
vi.mock('@tauri-apps/plugin-shell', () => shellMocks);

import {
  approveTemplateSetup,
  RUNTIME_JOB_EVENT_NAME,
  beginStudentWorkflow,
  cancelActiveJob,
  checkAppUpdate,
  closeCurrentProject,
  computeLmsBindingToken,
  confirmStudentAlignment,
  confirmStudentParseReview,
  deleteStudentSubmission,
  createProject,
  ensureLmsRosterPreload,
  finalizeReadyResults,
  generateQuestionRubric,
  getDefaultProjectsRoot,
  getExamWorkspaceState,
  getJobTrace,
  getLmsRosterCacheState,
  getShellState,
  intakeDefaultPdfRectsFromTemplate,
  listCanvasCourses,
  listCanvasCourseRoster,
  listLmsAssignmentsForCourse,
  listJobTraces,
  listVisionModels,
  validateVisionModel,
  openProject,
  previewResultsLmsReport,
  priorCanonicalSubmissionExistsForLmsStudent,
  projectExists,
  reanalyzeQuestion,
  replaceTemplatePdf,
  exportStampedTemplatePdf,
  getAppVersion,
  resolveLmsStudentRef,
  listenRuntimeJobEvents,
  runStudentIntake,
  runSmokePing,
  saveCriterionScore,
  saveModeratedFeedback,
  saveModeratedScore,
  saveProjectConfig,
  saveQuestionEdits,
  saveRedactionRegions,
  saveResultsLmsAssignment,
  saveRubricUpdate,
  setModerationQuestionReviewed,
  setSubmissionResultFinalized,
  skipTemplateRedaction,
  startJob,
  transientClipPdfRectsPngBase64,
  transientPdfClipText,
  transientRenderPdfPagePng,
  transientScansOcrHint,
  runResultsExport,
  runResultsLmsUpload,
  retryResultsLmsUpload,
  openExternalUrl,
  toDesktopAssetUrl
} from './desktop';

function createInput() {
  return {
    displayName: 'Midterm 1',
    subject: null,
    courseCode: null,
    lmsCourseId: null,
    projectRoot: null,
    templatePdfPath: '/tmp/template.pdf',
    instructorProfile: undefined
  };
}

describe('desktop API wrapper', () => {
  beforeEach(() => {
    coreMocks.invoke.mockReset();
    coreMocks.isTauri.mockReset();
    coreMocks.convertFileSrc.mockClear();
    eventMocks.listen.mockReset();
    appMocks.getVersion.mockReset();
    shellMocks.open.mockReset();
    appSettings.save(defaultAppSettings);
  });

  it('returns the synthetic browser shell state outside Tauri', async () => {
    coreMocks.isTauri.mockReturnValue(false);

    await expect(getShellState()).resolves.toEqual({
      currentProject: null,
      workerStatus: 'error',
      workerActivity: { activeJobs: [], pendingJobCount: 0 },
      debugFeatures: { redactionToggle: false },
      lastRuntimeError:
        'Desktop commands require the Tauri host. The browser preview cannot run Rust or Python commands.'
    });
    expect(coreMocks.invoke).not.toHaveBeenCalled();
  });

  it('rejects backend-only actions outside Tauri', async () => {
    coreMocks.isTauri.mockReturnValue(false);

    expect(() =>
      createProject(createInput(), { llmProvider: 'ollama_native', llmBaseUrl: '', llmModel: '', llmApiKey: null, lmsProvider: 'none', lmsCanvasBaseUrl: '', lmsCanvasApiKey: null,
        lmsBindingSecretPlaintextFallback: false,
        piiPaddleModelDir: null,
        preliminaryGradingMaxWorkers: 1,
        projectsDirectory: null,
        instructorProfile: { gradingStrictness: 'balanced', syntaxLeniency: 'medium', ocrTolerance: 'medium', partialCreditStyle: 'balanced', feedbackStyle: 'brief', enabledTags: { gradingStrictness: true, syntaxLeniency: false, ocrTolerance: false, partialCreditStyle: false, feedbackStyle: true }, additionalGuidance: '', includeMinimumCreditCriterion: false, minimumCreditPercent: 10 }, aiAssistEnabled: false, onboardingCompleted: false, aiAssistCategories: { rubrics: false, questionAnalysis: false, gradingFeedback: false, parsingReview: false }, theme: 'dark' })
    ).toThrow(
      'Desktop commands require the Tauri host. The browser preview cannot run Rust or Python commands.'
    );

    expect(() => listVisionModels('ollama_native', 'http://127.0.0.1:11434')).toThrow(
      'Desktop commands require the Tauri host. The browser preview cannot run Rust or Python commands.'
    );
    expect(() =>
      validateVisionModel('ollama_native', 'http://127.0.0.1:11434', 'qwen2.5vl:7b')
    ).toThrow(
      'Desktop commands require the Tauri host. The browser preview cannot run Rust or Python commands.'
    );

    expect(() => runSmokePing()).toThrow(
      'Desktop commands require the Tauri host. The browser preview cannot run Rust or Python commands.'
    );
  });

  it('subscribes to runtime job events through the Tauri event bridge', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    const observed: unknown[] = [];
    const unlisten = vi.fn();
    eventMocks.listen.mockImplementation(async (_eventName, handler) => {
      handler({
        payload: {
          eventType: 'job_finished',
          commandName: 'smoke.ping',
          workerStatus: 'ready',
          requestId: 'req_1',
          jobId: 'job_1',
          payload: { ok: true }
        }
      });
      return unlisten;
    });

    const returnedUnlisten = await listenRuntimeJobEvents((event) => {
      observed.push(event);
    });

    expect(eventMocks.listen).toHaveBeenCalledWith(
      RUNTIME_JOB_EVENT_NAME,
      expect.any(Function)
    );
    expect(observed).toEqual([
      {
        eventType: 'job_finished',
        commandName: 'smoke.ping',
        workerStatus: 'ready',
        requestId: 'req_1',
        jobId: 'job_1',
        payload: { ok: true }
      }
    ]);
    expect(returnedUnlisten).toBe(unlisten);
  });

  it('uses Tauri asset URLs only when running inside the desktop host', () => {
    coreMocks.isTauri.mockReturnValue(false);
    expect(toDesktopAssetUrl('/tmp/example.png')).toBe('/tmp/example.png');

    coreMocks.isTauri.mockReturnValue(true);
    expect(toDesktopAssetUrl('/tmp/example.png')).toBe('asset:///tmp/example.png');
    expect(coreMocks.convertFileSrc).toHaveBeenCalledWith('/tmp/example.png');
  });

  it('reads the packaged app version only inside the desktop host', async () => {
    coreMocks.isTauri.mockReturnValue(false);
    await expect(getAppVersion()).resolves.toBeNull();
    expect(appMocks.getVersion).not.toHaveBeenCalled();

    coreMocks.isTauri.mockReturnValue(true);
    appMocks.getVersion.mockResolvedValue('0.1.0-rc.1');
    await expect(getAppVersion()).resolves.toBe('0.1.0-rc.1');
  });

  it('checks for stable app updates through the desktop host with browser fallback', async () => {
    coreMocks.isTauri.mockReturnValue(false);
    await expect(checkAppUpdate()).resolves.toEqual(
      expect.objectContaining({
        installedVersion: 'Browser preview',
        status: 'unavailable',
        updateAvailable: false
      })
    );
    expect(coreMocks.invoke).not.toHaveBeenCalled();

    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValueOnce({
      installedVersion: '0.1.0-rc.1',
      latestStableVersion: '0.1.0',
      latestStableTag: 'v0.1.0',
      releaseUrl: 'https://github.com/ScriptScore/scriptscore/releases/tag/v0.1.0',
      updateAvailable: true,
      status: 'update_available',
      message: 'A newer stable ScriptScore Desktop release is available.'
    });

    await checkAppUpdate();

    expect(coreMocks.invoke).toHaveBeenCalledWith('check_app_update', {});
  });

  it('opens external update URLs through the shell plugin inside Tauri', async () => {
    coreMocks.isTauri.mockReturnValue(true);

    await openExternalUrl('https://github.com/ScriptScore/scriptscore/releases/tag/v0.1.0');

    expect(shellMocks.open).toHaveBeenCalledWith(
      'https://github.com/ScriptScore/scriptscore/releases/tag/v0.1.0'
    );
  });

  it('passes through Canvas and project query wrappers', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValueOnce([{ id: 'course_1' }]);
    coreMocks.invoke.mockResolvedValueOnce([{ userId: 'user_1' }]);
    coreMocks.invoke.mockResolvedValueOnce([{ assignmentId: 'assignment_1' }]);
    coreMocks.invoke.mockResolvedValueOnce('/tmp/projects');
    coreMocks.invoke.mockResolvedValueOnce(true);
    coreMocks.invoke.mockResolvedValueOnce({ project: { projectId: 'proj_1' } });

    await listCanvasCourses('https://canvas.example.test', 'token');
    await listCanvasCourseRoster('https://canvas.example.test', 'token', 'course_1');
    await listLmsAssignmentsForCourse('course_1');
    await getDefaultProjectsRoot();
    await projectExists('/tmp/project');
    await getExamWorkspaceState();

    expect(coreMocks.invoke).toHaveBeenNthCalledWith(1, 'list_canvas_courses', {
      baseUrl: 'https://canvas.example.test',
      accessToken: 'token'
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(2, 'list_canvas_course_roster', {
      baseUrl: 'https://canvas.example.test',
      accessToken: 'token',
      courseId: 'course_1'
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(3, 'list_lms_assignments_for_course', {
      courseId: 'course_1',
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(4, 'get_default_projects_root', {});
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(5, 'project_exists', {
      projectPath: '/tmp/project'
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(6, 'get_exam_workspace_state', {});
  });

  it('injects current app settings for LMS token helpers', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    appSettings.save({
      ...defaultAppSettings,
      lmsProvider: 'canvas',
      lmsCanvasBaseUrl: 'https://canvas.example.test',
      lmsCanvasApiKey: 'token',
      lmsBindingSecretPlaintextFallback: true
    });

    coreMocks.invoke.mockResolvedValue('token_42');

    await computeLmsBindingToken('course_1', 'user_1');
    await priorCanonicalSubmissionExistsForLmsStudent('course_1', 'user_1');
    await resolveLmsStudentRef('course_1', 'user_1');

    expect(coreMocks.invoke).toHaveBeenNthCalledWith(
      1,
      'compute_lms_binding_token',
      expect.objectContaining({
        courseId: 'course_1',
        canvasUserId: 'user_1',
        settings: expect.objectContaining({
          lmsProvider: 'canvas',
          lmsBindingSecretPlaintextFallback: true
        })
      })
    );
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(
      2,
      'prior_canonical_submission_exists_for_lms_student',
      expect.objectContaining({
        courseId: 'course_1',
        canvasUserId: 'user_1',
        settings: expect.objectContaining({
          lmsProvider: 'canvas',
          lmsBindingSecretPlaintextFallback: true
        })
      })
    );
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(
      3,
      'resolve_lms_student_ref',
      expect.objectContaining({
        courseId: 'course_1',
        canvasUserId: 'user_1',
        settings: expect.objectContaining({
          lmsProvider: 'canvas',
          lmsBindingSecretPlaintextFallback: true
        })
      })
    );
  });

  it('invokes student workflow and runtime wrappers with the expected payloads', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValue('job_1');
    const workflowSettings = {
      ...defaultAppSettings,
      preliminaryGradingMaxWorkers: 3
    };

    await transientPdfClipText('/tmp/exam.pdf', 1, 1, 2, 3, 4);
    await intakeDefaultPdfRectsFromTemplate('/tmp/exam.pdf');
    await beginStudentWorkflow(workflowSettings);
    await confirmStudentParseReview('student_1', 'question_1', 'answer text', defaultAppSettings);
    await deleteStudentSubmission({ studentRef: 'student_1' });
    await runStudentIntake([
      {
        studentRef: 'student_1',
        rawPdfPath: '/tmp/exam.pdf',
        desiredPageOrder: [2, 1],
        redactionRegionsPx: [{ pageNumber: 1, x: 10, y: 20, width: 30, height: 40 }],
        rasterSizesByPage: { 1: { widthPx: 600, heightPx: 800 } }
      }
    ]);
    await startJob('smoke.ping', { ok: true });
    await cancelActiveJob('job_1');
    await getJobTrace('job_1', 'smoke.ping');

    expect(coreMocks.invoke).toHaveBeenNthCalledWith(1, 'transient_pdf_clip_text', {
      input: {
        pdfPath: '/tmp/exam.pdf',
        pageNumber: 1,
        xPt: 1,
        yPt: 2,
        widthPt: 3,
        heightPt: 4
      }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(
      2,
      'intake_default_pdf_rects_from_template',
      { pdfPath: '/tmp/exam.pdf' }
    );
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(3, 'begin_student_workflow', {
      settings: expect.objectContaining({ preliminaryGradingMaxWorkers: 3 })
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(4, 'confirm_student_parse_review', {
      input: {
        studentRef: 'student_1',
        questionId: 'question_1',
        correctedText: 'answer text'
      },
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(5, 'delete_student_submission', {
      input: { studentRef: 'student_1' }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(6, 'run_student_intake', {
      inputs: [
        {
          studentRef: 'student_1',
          rawPdfPath: '/tmp/exam.pdf',
          desiredPageOrder: [2, 1],
          redactionRegionsPx: [{ pageNumber: 1, x: 10, y: 20, width: 30, height: 40 }],
          rasterSizesByPage: { 1: { widthPx: 600, heightPx: 800 } }
        }
      ]
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(7, 'start_job', {
      commandName: 'smoke.ping',
      requestPayload: { ok: true }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(8, 'cancel_active_job', {
      jobId: 'job_1'
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(9, 'get_job_trace', {
      jobId: 'job_1',
      commandName: 'smoke.ping'
    });
    await listJobTraces();
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(10, 'list_job_traces', {});
  });

  it('invokes the criterion score save wrapper with the expected payload', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValue({ project: { projectId: 'proj_1' } });

    await saveCriterionScore({
      studentRef: 'student_1',
      questionId: 'question_1',
      criterionIndex: 2,
      pointsAwarded: 1
    });

    expect(coreMocks.invoke).toHaveBeenCalledWith('save_criterion_score', {
      input: {
        studentRef: 'student_1',
        questionId: 'question_1',
        criterionIndex: 2,
        pointsAwarded: 1
      }
    });
  });

  it('invokes moderation wrappers with the expected payloads', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValue({ project: { projectId: 'proj_1' } });

    await saveModeratedScore({
      studentRef: 'student_1',
      questionId: 'question_1',
      moderatedTotalPoints: 3
    });
    await saveModeratedFeedback({
      studentRef: 'student_1',
      questionId: 'question_1',
      feedbackText: 'Edited feedback'
    });
    await setModerationQuestionReviewed({
      questionId: 'question_1',
      reviewed: true
    });

    expect(coreMocks.invoke).toHaveBeenNthCalledWith(1, 'save_moderated_score', {
      input: {
        studentRef: 'student_1',
        questionId: 'question_1',
        moderatedTotalPoints: 3
      }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(2, 'save_moderated_feedback', {
      input: {
        studentRef: 'student_1',
        questionId: 'question_1',
        feedbackText: 'Edited feedback'
      }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(3, 'set_moderation_question_reviewed', {
      input: {
        questionId: 'question_1',
        reviewed: true
      }
    });
  });

  it('invokes Results wrappers with the expected payloads', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValue({ project: { projectId: 'proj_1' } });

    await saveResultsLmsAssignment({ assignmentId: 'assignment_1' });
    await setSubmissionResultFinalized({ studentRef: 'student_1', finalized: true });
    await finalizeReadyResults({ studentRefs: ['student_1'] });
    await previewResultsLmsReport('student_1');
    await runResultsLmsUpload({ mode: 'live', studentRefs: ['student_1'] });
    await retryResultsLmsUpload({ attemptId: 'attempt_1' });
    await runResultsExport({
      format: 'csv',
      studentRefs: ['student_1'],
      destinationPath: '/tmp/results.csv'
    });

    expect(coreMocks.invoke).toHaveBeenNthCalledWith(1, 'save_results_lms_assignment', {
      input: { assignmentId: 'assignment_1' },
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(2, 'set_submission_result_finalized', {
      input: { studentRef: 'student_1', finalized: true }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(3, 'finalize_ready_results', {
      input: { studentRefs: ['student_1'] }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(4, 'preview_results_lms_report', {
      studentRef: 'student_1'
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(5, 'run_results_lms_upload', {
      input: { mode: 'live', studentRefs: ['student_1'] },
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(6, 'retry_results_lms_upload', {
      input: { attemptId: 'attempt_1' },
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(7, 'run_results_export', {
      input: {
        format: 'csv',
        studentRefs: ['student_1'],
        destinationPath: '/tmp/results.csv'
      }
    });
  });

  it('rounds fractional student intake geometry before run_student_intake invoke', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValue('job_1');

    await runStudentIntake([
      {
        studentRef: 'student_1',
        rawPdfPath: '/tmp/exam.pdf',
        desiredPageOrder: [2 + 1e-10, 1 + 1e-10],
        redactionRegionsPx: [
          { pageNumber: 1, x: 10.4, y: 20.6, width: 619.1999999999999, height: 40.2 }
        ],
        rasterSizesByPage: { 1: { widthPx: 600.7, heightPx: 800.3 } }
      }
    ]);

    expect(coreMocks.invoke).toHaveBeenCalledWith('run_student_intake', {
      inputs: [
        {
          studentRef: 'student_1',
          rawPdfPath: '/tmp/exam.pdf',
          desiredPageOrder: [2, 1],
          redactionRegionsPx: [
            { pageNumber: 1, x: 10, y: 21, width: 619, height: 40 }
          ],
          rasterSizesByPage: { 1: { widthPx: 601, heightPx: 800 } }
        }
      ]
    });
  });

  it('covers the remaining project and student wrapper commands', async () => {
    coreMocks.isTauri.mockReturnValue(true);
    coreMocks.invoke.mockResolvedValue({});

    await openProject('/tmp/project');
    await closeCurrentProject();
    await runSmokePing();
    await listVisionModels('ollama_native', 'http://127.0.0.1:11434', 'secret');
    await validateVisionModel('ollama_cloud', 'https://ollama.com/api', 'qwen2.5vl:7b', 'secret');
    await saveQuestionEdits([{ questionId: 'question_1', questionNumber: 1, pageNumber: 1, maxPoints: 5, text: 'Question one', questionContext: '' }]);
    await saveRedactionRegions([{ regionId: 'region_1', pageNumber: 1, x: 1, y: 2, width: 3, height: 4 }]);
    await approveTemplateSetup(defaultAppSettings);
    await getLmsRosterCacheState();
    await ensureLmsRosterPreload();
    await saveProjectConfig({
      projectId: 'proj_1',
      displayName: 'Midterm 1',
      subject: 'Chemistry',
      courseCode: 'CHEM 201',
      lmsCourseId: null,
      redactionRequired: true,
      instructorProfile: structuredClone(defaultAppSettings.instructorProfile),
      traceRefs: {
        setupJobId: null,
        batchAnalyzeJobId: null,
        batchRubricJobId: null,
        intakeJobId: null
      },
      createdAt: '1',
      updatedAt: '1'
    }, defaultAppSettings);
    await skipTemplateRedaction();
    await generateQuestionRubric('question_1', true, defaultAppSettings);
    await reanalyzeQuestion('question_1', defaultAppSettings);
    await saveRubricUpdate({ questionId: 'question_1', criteria: [], approve: true });
    await replaceTemplatePdf('/tmp/template.pdf');
    await exportStampedTemplatePdf('/tmp/template-stamped.pdf');
    await transientRenderPdfPagePng('/tmp/exam.pdf', 2, 1.5, 1600);
    await transientClipPdfRectsPngBase64('/tmp/exam.pdf', [{ pageNumber: 2, xPt: 1, yPt: 2, widthPt: 3, heightPt: 4 }], 1.5);
    await transientScansOcrHint('ZmFrZQ==');
    await confirmStudentAlignment(
      'student_1',
      [
        {
          pageNumber: 1,
          confidence: 0.9,
          lowConfidence: false,
          transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 },
          warnings: []
        }
      ],
      defaultAppSettings
    );

    expect(coreMocks.invoke).toHaveBeenCalledWith('open_project', {
      projectPath: '/tmp/project',
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('close_current_project', {});
    expect(coreMocks.invoke).toHaveBeenCalledWith('run_smoke_ping', {});
    expect(coreMocks.invoke).toHaveBeenCalledWith('list_llm_models', {
      providerName: 'ollama_native',
      baseUrl: 'http://127.0.0.1:11434',
      apiKey: 'secret'
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('validate_llm_model', {
      providerName: 'ollama_cloud',
      baseUrl: 'https://ollama.com/api',
      model: 'qwen2.5vl:7b',
      apiKey: 'secret'
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('save_question_edits', expect.any(Object));
    expect(coreMocks.invoke).toHaveBeenCalledWith('save_redaction_regions', expect.any(Object));
    expect(coreMocks.invoke).toHaveBeenCalledWith('approve_template_setup', {
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('get_lms_roster_cache_state', {
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('ensure_lms_roster_preload', {
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith(
      'save_project_config',
      expect.objectContaining({
        settings: defaultAppSettings
      })
    );
    expect(coreMocks.invoke).toHaveBeenCalledWith('skip_template_redaction', {});
    expect(coreMocks.invoke).toHaveBeenCalledWith('generate_question_rubric', expect.any(Object));
    expect(coreMocks.invoke).toHaveBeenCalledWith('reanalyze_question', {
      questionId: 'question_1',
      settings: defaultAppSettings
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('save_rubric_update', expect.any(Object));
    expect(coreMocks.invoke).toHaveBeenCalledWith('replace_template_pdf', {
      templatePdfPath: '/tmp/template.pdf'
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('export_stamped_template_pdf', {
      destinationPath: '/tmp/template-stamped.pdf'
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith('transient_render_pdf_page_png', {
      pdfPath: '/tmp/exam.pdf',
      pageNumber: 2,
      zoom: 1.5,
      maxWidthPx: 1600
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith(
      'transient_clip_pdf_rects_png_base64',
      expect.any(Object)
    );
    expect(coreMocks.invoke).toHaveBeenCalledWith('transient_scans_ocr_hint', {
      pngBytesBase64: 'ZmFrZQ=='
    });
    expect(coreMocks.invoke).toHaveBeenCalledWith(
      'confirm_student_alignment',
      expect.objectContaining({
        input: expect.objectContaining({ studentRef: 'student_1' }),
        settings: defaultAppSettings
      })
    );
  });

  it('omits the transient render width cap when none is supplied', async () => {
    coreMocks.isTauri.mockReturnValue(true);

    await transientRenderPdfPagePng('/tmp/exam.pdf', 2, 1.5);

    expect(coreMocks.invoke).toHaveBeenCalledWith('transient_render_pdf_page_png', {
      pdfPath: '/tmp/exam.pdf',
      pageNumber: 2,
      zoom: 1.5
    });
  });
});
