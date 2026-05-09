<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';

  import NoProjectScreen from '$lib/components/desktop/NoProjectScreen.svelte';
  import type { NoProjectScreenProps } from '$lib/components/desktop/NoProjectScreen.types';
  import ModerationWorkspace from '$lib/components/desktop/ModerationWorkspace.svelte';
  import ProjectRail from '$lib/components/desktop/ProjectRail.svelte';
  import ProjectTopBar from '$lib/components/desktop/ProjectTopBar.svelte';
  import ResultsWorkspace from '$lib/components/desktop/ResultsWorkspace.svelte';
  import SetupWorkspace from '$lib/components/desktop/SetupWorkspace.svelte';
  import ReviewWorkspace from '$lib/components/desktop/ReviewWorkspace.svelte';
  import SettingsWorkspace from '$lib/components/desktop/SettingsWorkspace.svelte';
  import StudentWorkflowWorkspace from '$lib/components/desktop/StudentWorkflowWorkspace.svelte';
  import TraceHistoryDialog from '$lib/components/desktop/TraceHistoryDialog.svelte';
  import WorkspaceMessages from '$lib/components/desktop/WorkspaceMessages.svelte';
  import NotificationToasts from '$lib/components/desktop/NotificationToasts.svelte';
  import { ConfirmDialog, DesktopButton, Surface } from '$lib/components/desktop/ui';
  import {
    cancelActiveJob,
    closeCurrentProject,
    computeLmsBindingToken,
    createProject,
    beginStudentWorkflow,
    confirmStudentAlignment,
    confirmStudentDetectReview,
    confirmStudentParseReview,
    deleteStudentSubmission,
    finalizeReadyResults,
    generateQuestionRubric,
    getJobTrace,
    getDefaultProjectsRoot,
    getExamWorkspaceState,
    getLmsRosterCacheState,
    isDesktopHost,
    listJobTraces,
    listVisionModels,
    openProject,
    previewResultsLmsReport,
    projectExists,
    retryResultsLmsUpload,
    exportStampedTemplatePdf,
    replaceTemplatePdf,
    reanalyzeQuestion,
    runSmokePing,
    runResultsExport,
    runResultsLmsUpload,
    saveQuestionEdits,
    saveProjectConfig,
    saveResultsLmsAssignment,
    saveRubricUpdate,
    saveRedactionRegions,
    saveCriterionScore,
    saveStudentAlignmentReview,
    saveStudentDetectReview,
    saveStudentParseReview,
    saveModeratedFeedback,
    saveModeratedScore,
    setModerationQuestionReviewed,
    setSubmissionResultFinalized,
    resolveLmsStudentRef,
    runStudentIntake
  } from '$lib/desktop';
  import {
    ensureRuntimeJobBridge,
    jobProgress,
    onRuntimeJobEvent,
    onJobCompletion,
    refreshShellState,
    shellState,
    teardownRuntimeJobBridge
  } from '$lib/stores/shell';
  import { theme } from '$lib/stores/theme';
  import { scheduleAutomaticRubricEnsureAfterUiPaint } from '$lib/automaticRubricEnsure';
  import { appSettings, defaultAppSettings, isCanvasLmsReady } from '$lib/stores/appSettings';
  import { notifications } from '$lib/stores/notifications';
  import {
    type BusyAction,
    type TemplateSetupSubstep,
    workspaceView
  } from '$lib/stores/workspaceView';
  import { resultsWorkspaceView } from '$lib/stores/resultsWorkspaceView';
  import type {
    AppSettings,
    CreateProjectInput,
    ExamWorkspaceState,
    ProjectConfig,
    QuestionEdit,
    RubricCriterion,
    RuntimeJobEvent,
    ShellState,
    SmokePingResult,
    StudentIntakeFinalizePayload,
    StudentIntakeFinalizeResult,
    StudentIntakeInput,
    TemplateRedactionRegionInput,
    ResultsExportFormat,
    VisionCapableModel
  } from '$lib/types';

  type ApprovedEditImpact = 'minor' | 'grading';
  type PendingOperatorConfirmation = {
    title: string;
    message: string;
    confirmLabel: string;
    cancelLabel: string;
    destructive: boolean;
    resolve: (confirmed: boolean) => void;
  };
  import { isTemplateRedactionsReadyForReview } from '$lib/templateSetupRail';
  import { studentIntakePrerequisitesMet } from '$lib/studentIntakePrerequisites';
  import { applyResultsUploadRuntimeEvent } from '$lib/resultsUploadRuntimeEvents';
  import { resolveStudentDisplayNamesForWorkspace } from '$lib/resultsWorkspaceContext';
  import {
    hostWorkflowData,
    hostWorkflowErrorMessage
  } from '$lib/hostWorkflowCompletion';
  import type { AnalysisJobState } from '$lib/reviewSidebarState';
  import {
    shouldEnsureAutomaticRubricsAfterTerminalJob,
    shouldRefreshWorkspaceAfterTerminalJob,
    shouldRefreshWorkspaceDuringRuntimeEvent
  } from '$lib/workspaceRefreshPolicy';
  import {
    analysisActiveState,
    analysisEventIsTerminal,
    hasRunnableStudentWorkflowRows,
    moderationNeedsAttention,
    resultsNeedAttention,
    runtimeEventQuestionId,
    runtimeJobKey,
    setStudentReviewSaveBusyState,
    studentReviewSaveKey
  } from '$lib/desktopRouteState';

  let showCreateForm = false;
  let showLandingSettings = false;
  let forceOnboardingOpen = false;
  let busyAction: BusyAction = null;
  let actionError: string | null = null;
  let runtimeResult: SmokePingResult | null = null;
  let workspaceState: ExamWorkspaceState | null = null;
  let questionDrafts: QuestionEdit[] = [];
  let projectConfigDraft: ProjectConfig | null = null;
  let workspaceRefreshInFlight = false;
  let workspaceRefreshQueued = false;
  let settingsDraft: AppSettings = structuredClone(defaultAppSettings);
  let intakeDrafts: StudentIntakeInput[] = [];
  let visionModels: VisionCapableModel[] = [];
  let visionModelsBusy = false;
  let visionModelsError: string | null = null;
  let defaultProjectsRoot: string | null = null;
  let lastVisionModelQueryKey: string | null = null;
  let visionModelRefreshInFlight = false;
  let visionModelRefreshQueued = false;
  let visionModelRefreshVersion = 0;
  let studentDisplayNamesByRef: Record<string, string> = {};
  let studentNameLookupVersion = 0;
  let activeWorkspaceJobId: string | null = null;
  let stopWorkflowBusy = false;
  let studentReviewSaveBusy: Record<string, boolean> = {};
  let deferredStudentWorkflowContinuation = false;
  let studentWorkflowContinuationQueued = false;
  let activeAnalysisJobKeys = new Set<string>();
  let activeAnalysisQuestionByJobKey = new Map<string, string>();
  let activeAnalysisJobByQuestion: Record<string, AnalysisJobState> = {};
  let targetedAnalysisInFlightCount = 0;
  let targetedUnscopedAnalysisJobKeys = new Set<string>();
  let alignmentStampJobActive = false;
  let pendingOperatorConfirmation: PendingOperatorConfirmation | null = null;
  let shownRecoveryRuntimeNotices = new Set<string>();
  let visibleRuntimeError: string | null = null;
  let traceHistoryOpen = false;
  let traceHistoryInitialJobIds: string[] = [];
  let traceHistoryInitialStateFilter = '';

  const hasDesktopHost = isDesktopHost();
  $: analysisInProgress = activeAnalysisJobKeys.size > 0;

  let createInput: CreateProjectInput = {
    displayName: '',
    subject: '',
    courseCode: '',
    lmsCourseId: null,
    projectRoot: null,
    templatePdfPath: '',
    instructorProfile: undefined
  };

  async function loadRecentProjects(): Promise<NoProjectScreenProps['recentProjects']> {
    if (typeof window === 'undefined') return [];
    try {
      const raw = localStorage.getItem('scriptscore-recent-projects');
      if (!raw) return [];
      const parsed = JSON.parse(raw) as Array<{ projectPath: string; displayName: string; courseCode: string | null; openedAt: string }>;
      const valid: typeof parsed = [];
      const stale: string[] = [];
      for (const project of parsed) {
        const exists = hasDesktopHost ? await projectExists(project.projectPath) : true;
        if (exists) {
          valid.push(project);
        } else {
          stale.push(project.projectPath);
        }
      }
      if (stale.length > 0) {
        localStorage.setItem('scriptscore-recent-projects', JSON.stringify(valid.slice(0, 6)));
      }
      return valid.slice(0, 6);
    } catch {
      return [];
    }
  }

  function saveRecentProject(projectPath: string, displayName: string, courseCode: string | null) {
    if (typeof window === 'undefined') return;
    try {
      const existing = JSON.parse(localStorage.getItem('scriptscore-recent-projects') || '[]') as Array<{ projectPath: string; displayName: string; courseCode: string | null; openedAt: string }>;
      const filtered = existing.filter((p) => p.projectPath !== projectPath);
      const next = [
        { projectPath, displayName, courseCode, openedAt: new Date().toISOString() },
        ...filtered
      ].slice(0, 6);
      localStorage.setItem('scriptscore-recent-projects', JSON.stringify(next));
      recentProjects = next;
    } catch {
      // ignore
    }
  }

  let recentProjects: NoProjectScreenProps['recentProjects'] = [];
  if (hasDesktopHost) {
    loadRecentProjects().then((projects) => {
      recentProjects = projects;
    });
  }

  function normalizeQuestionContext(raw: string | null | undefined): string {
    return raw ?? '';
  }

  function rubricIsApproved(
    rubric: ExamWorkspaceState['questions'][number]['rubric'] | null | undefined
  ): boolean {
    return rubric?.status === 'approved' || rubric?.approvedAt != null;
  }

  async function confirmOperatorAction(message: string, title = 'Confirm action'): Promise<boolean> {
    if (typeof window === 'undefined') {
      return true;
    }
    pendingOperatorConfirmation?.resolve(false);
    return new Promise((resolve) => {
      pendingOperatorConfirmation = {
        title,
        message,
        confirmLabel: 'Confirm',
        cancelLabel: 'Cancel',
        destructive: false,
        resolve
      };
    });
  }

  function resolveOperatorConfirmation(confirmed: boolean): void {
    const pending = pendingOperatorConfirmation;
    if (!pending) return;
    pendingOperatorConfirmation = null;
    pending.resolve(confirmed);
  }

  function isRecoveredStaleJobNotice(message: string | null): message is string {
    return /^Recovered \d+ stale desktop job records? from a prior session\.$/.test(
      message ?? ''
    );
  }

  function applyAlignmentStampRuntimeEvent(event: RuntimeJobEvent): void {
    if (event.commandName !== 'scans.pdf-stamp-aruco') {
      return;
    }
    if (
      event.eventType === 'job_queued' ||
      event.eventType === 'job_submitted' ||
      event.eventType === 'job_started' ||
      event.eventType === 'job_progress'
    ) {
      alignmentStampJobActive = true;
      return;
    }
    if (event.eventType === 'job_failed' || event.eventType === 'job_cancelled') {
      alignmentStampJobActive = false;
    }
  }

  function rememberTargetedUnscopedAnalysisJob(key: string): void {
    targetedUnscopedAnalysisJobKeys = new Set(targetedUnscopedAnalysisJobKeys).add(key);
  }

  function forgetTargetedUnscopedAnalysisJob(key: string): void {
    if (!targetedUnscopedAnalysisJobKeys.has(key)) {
      return;
    }
    const nextTargetedKeys = new Set(targetedUnscopedAnalysisJobKeys);
    nextTargetedKeys.delete(key);
    targetedUnscopedAnalysisJobKeys = nextTargetedKeys;
  }

  function setQuestionAnalysisJobState(questionId: string, state: AnalysisJobState): void {
    activeAnalysisJobByQuestion = {
      ...activeAnalysisJobByQuestion,
      [questionId]: state
    };
  }

  function clearQuestionAnalysisJobState(questionId: string): void {
    if (!activeAnalysisJobByQuestion[questionId]) {
      return;
    }
    const next = { ...activeAnalysisJobByQuestion };
    delete next[questionId];
    activeAnalysisJobByQuestion = next;
  }

  function applyAnalysisRuntimeEvent(event: RuntimeJobEvent): void {
    if (event.commandName !== 'exam.analyze') {
      return;
    }
    const key = runtimeJobKey(event);
    if (!key) {
      return;
    }
    const nextGlobalKeys = new Set(activeAnalysisJobKeys);
    const questionId = runtimeEventQuestionId(event) ?? activeAnalysisQuestionByJobKey.get(key) ?? null;
    const targetedUnscoped =
      questionId === null &&
      (targetedAnalysisInFlightCount > 0 || targetedUnscopedAnalysisJobKeys.has(key));
    const activeState = analysisActiveState(event);
    if (activeState !== null) {
      if (targetedUnscoped) {
        rememberTargetedUnscopedAnalysisJob(key);
        activeAnalysisJobKeys = nextGlobalKeys;
        return;
      }
      if (questionId) {
        activeAnalysisQuestionByJobKey.set(key, questionId);
        setQuestionAnalysisJobState(questionId, activeState);
      } else {
        nextGlobalKeys.add(key);
      }
    }
    if (analysisEventIsTerminal(event)) {
      nextGlobalKeys.delete(key);
      forgetTargetedUnscopedAnalysisJob(key);
      activeAnalysisQuestionByJobKey.delete(key);
      if (questionId) {
        clearQuestionAnalysisJobState(questionId);
      }
    }
    activeAnalysisJobKeys = nextGlobalKeys;
  }

  $: orderedQuestionDrafts = [...questionDrafts].sort(
    (left, right) =>
      left.questionNumber - right.questionNumber ||
      left.pageNumber - right.pageNumber ||
      left.questionId.localeCompare(right.questionId)
  );
  $: hasUnsavedQuestionEdits =
    workspaceState !== null &&
    JSON.stringify(questionDrafts) !==
      JSON.stringify(
        workspaceState.questions.map((question) => ({
          questionId: question.questionId,
          questionNumber: question.questionNumber,
          pageNumber: question.pageNumber,
          maxPoints: question.maxPoints,
          text: question.text,
          questionContext: normalizeQuestionContext(question.analysis?.questionContext ?? undefined)
        }))
      );
  $: attentionByStep =
    workspaceState === null
      ? {}
      : {
          templateSetup:
            workspaceState.workflowStage === 'redaction_review' ||
            workspaceState.workflowStage === 'question_review' ||
            workspaceState.workflowStage === 'rubric_authoring',
          students:
            workspaceState.workflowStage === 'student_intake_ready' ||
            workspaceState.workflowStage === 'student_workflow_running' ||
            workspaceState.workflowStage === 'student_workflow_review' ||
            workspaceState.workflowStage === 'student_grading' ||
            workspaceState.workflowStage === 'student_grading_complete',
          moderation: moderationNeedsAttention(workspaceState),
          exportResults: resultsNeedAttention(workspaceState)
        };

  $: documentTitle = $shellState.currentProject
    ? `ScriptScore Desktop - ${$shellState.currentProject.displayName}${
        $shellState.currentProject.courseCode ? ` · ${$shellState.currentProject.courseCode}` : ''
      }`
    : 'ScriptScore';
  $: appTitle = $shellState.currentProject
    ? $shellState.currentProject.displayName
    : 'ScriptScore Desktop';
  $: visibleRuntimeError = isRecoveredStaleJobNotice($shellState.lastRuntimeError)
    ? null
    : $shellState.lastRuntimeError;
  $: {
    const recoveryNotice = isRecoveredStaleJobNotice($shellState.lastRuntimeError)
      ? $shellState.lastRuntimeError
      : null;
    if (recoveryNotice && !shownRecoveryRuntimeNotices.has(recoveryNotice)) {
      notifications.pushInfo(recoveryNotice);
      shownRecoveryRuntimeNotices = new Set(shownRecoveryRuntimeNotices).add(recoveryNotice);
    }
  }
  onMount(() => {
    const stopBackgroundAnalysisRefresh = onJobCompletion({
      onFinished: (event) => {
        if (shouldRefreshWorkspaceAfterTerminalJob(event)) {
          requestWorkspaceRefresh(true, true);
        }
        if (shouldEnsureAutomaticRubricsAfterTerminalJob(event)) {
          scheduleAutomaticRubricEnsureAfterUiPaint(true);
        }
      },
      onFailed: (event) => {
        if (shouldRefreshWorkspaceAfterTerminalJob(event)) {
          requestWorkspaceRefresh(true, true);
        }
      }
    });
    const stopWorkflowRefresh = onRuntimeJobEvent((event) => {
      applyAnalysisRuntimeEvent(event);
      applyAlignmentStampRuntimeEvent(event);
      if (shouldRefreshWorkspaceDuringRuntimeEvent(event)) {
        requestWorkspaceRefresh(true, true);
      }
      handleRosterCacheRuntimeEvent(event);
      handleResultsLmsUploadRuntimeEvent(event);
    });

    theme.init();
    appSettings.init();
    void (async () => {
      await ensureRuntimeJobBridge();
      await refreshShellState();
      await loadDefaultProjectsRoot();
      await loadWorkspaceState();
    })();

    return () => {
      stopBackgroundAnalysisRefresh();
      stopWorkflowRefresh();
      teardownRuntimeJobBridge();
    };
  });

  $: if (typeof window !== 'undefined') {
    settingsDraft = structuredClone($appSettings);
  }

  $: noProjectOnboardingOpen =
    hasDesktopHost &&
    !$shellState.currentProject &&
    (forceOnboardingOpen || !$appSettings.onboardingCompleted);

  $: if (
    hasDesktopHost &&
    ($workspaceView.activeWorkflowStep === 'settings' || showLandingSettings || noProjectOnboardingOpen)
  ) {
    const queryKey = `${settingsDraft.llmProvider}|${settingsDraft.llmBaseUrl.trim()}`;
    if (queryKey !== lastVisionModelQueryKey) {
      lastVisionModelQueryKey = queryKey;
      void refreshVisionModels();
    }
  }

  $: resolvedProjectsDirectory = settingsDraft.projectsDirectory ?? defaultProjectsRoot;

  async function loadDefaultProjectsRoot() {
    if (!hasDesktopHost) {
      defaultProjectsRoot = null;
      return;
    }
    try {
      const root = await getDefaultProjectsRoot();
      defaultProjectsRoot = root || null;
    } catch {
      defaultProjectsRoot = null;
    }
  }

  async function refreshVisionModels() {
    if (!hasDesktopHost) {
      visionModels = [];
      visionModelsBusy = false;
      visionModelsError = null;
      visionModelRefreshQueued = false;
      return;
    }

    const baseUrl = settingsDraft.llmBaseUrl.trim();
    if (settingsDraft.llmProvider !== 'ollama_native') {
      visionModels = [];
      visionModelsBusy = false;
      visionModelsError = null;
      visionModelRefreshQueued = false;
      return;
    }

    if (!baseUrl) {
      visionModels = [];
      visionModelsBusy = false;
      visionModelsError = 'Enter an Ollama endpoint URL to load local models.';
      visionModelRefreshQueued = false;
      return;
    }

    if (visionModelRefreshInFlight) {
      visionModelRefreshQueued = true;
      return;
    }

    const requestVersion = ++visionModelRefreshVersion;
    const requestKey = `${settingsDraft.llmProvider}|${baseUrl}`;
    visionModelRefreshInFlight = true;
    visionModelsBusy = true;
    visionModelsError = null;
    try {
      const models = await listVisionModels(
        settingsDraft.llmProvider,
        baseUrl,
        settingsDraft.llmApiKey?.trim() || null
      );
      if (
        requestVersion !== visionModelRefreshVersion ||
        requestKey !== `${settingsDraft.llmProvider}|${settingsDraft.llmBaseUrl.trim()}`
      ) {
        return;
      }
      visionModels = models;
    } catch (error) {
      if (
        requestVersion !== visionModelRefreshVersion ||
        requestKey !== `${settingsDraft.llmProvider}|${settingsDraft.llmBaseUrl.trim()}`
      ) {
        return;
      }
      visionModels = [];
      visionModelsError = localOllamaDiscoveryErrorMessage(error);
    } finally {
      if (requestVersion === visionModelRefreshVersion) {
        visionModelsBusy = false;
      }
      visionModelRefreshInFlight = false;
      if (visionModelRefreshQueued) {
        visionModelRefreshQueued = false;
        void refreshVisionModels();
      }
    }
  }

  function localOllamaDiscoveryErrorMessage(error: unknown) {
    const message = String(error);
    if (message.includes('A desktop job is already active in this session.')) {
      return 'The desktop runtime is busy with another job. Wait for it to finish, then accept the Ollama URL again.';
    }
    return message;
  }

  async function loadWorkspaceState(resetQuestions = true, preserveProjectConfig = false) {
    if (!hasDesktopHost || !$shellState.currentProject) {
      workspaceState = null;
      questionDrafts = [];
      clearStudentDisplayNames();
      workspaceView.reset();
      return;
    }
    const next = await getExamWorkspaceState();
    applyWorkspaceState(next, resetQuestions, preserveProjectConfig);
  }

  async function refreshStudentDisplayNames(
    targetWorkspace: ExamWorkspaceState
  ): Promise<void> {
    const requestVersion = ++studentNameLookupVersion;
    try {
      const nextNames = await resolveStudentDisplayNamesForWorkspace(targetWorkspace, {
        getLmsRosterCacheState,
        computeLmsBindingToken
      });
      if (requestVersion !== studentNameLookupVersion) {
        return;
      }
      studentDisplayNamesByRef = nextNames;
    } catch {
      if (requestVersion === studentNameLookupVersion) {
        studentDisplayNamesByRef = {};
      }
    }
  }

  function clearStudentDisplayNames(): void {
    studentNameLookupVersion += 1;
    studentDisplayNamesByRef = {};
  }

  function openTraceHistory(): void {
    traceHistoryInitialJobIds = [];
    traceHistoryInitialStateFilter = '';
    traceHistoryOpen = true;
  }

  function openActiveTraceHistory(jobIds: string[]): void {
    traceHistoryInitialJobIds = [...jobIds];
    traceHistoryInitialStateFilter = jobIds.length > 0 ? 'running' : '';
    traceHistoryOpen = true;
  }

  function requestWorkspaceRefresh(resetQuestions = true, preserveProjectConfig = false) {
    if (workspaceRefreshInFlight) {
      workspaceRefreshQueued = true;
      return;
    }
    workspaceRefreshInFlight = true;
    void (async () => {
      try {
        await loadWorkspaceState(resetQuestions, preserveProjectConfig);
      } finally {
        workspaceRefreshInFlight = false;
        if (workspaceRefreshQueued) {
          workspaceRefreshQueued = false;
          requestWorkspaceRefresh(resetQuestions, preserveProjectConfig);
        }
      }
    })();
  }

  function toQuestionEdit(question: ExamWorkspaceState['questions'][number]): QuestionEdit {
    return {
      questionId: question.questionId,
      questionNumber: question.questionNumber,
      pageNumber: question.pageNumber,
      maxPoints: question.maxPoints,
      text: question.text,
      questionContext: normalizeQuestionContext(question.analysis?.questionContext ?? undefined)
    };
  }

  function applyWorkspaceState(next: ExamWorkspaceState, resetQuestions: boolean, preserveProjectConfig: boolean = false) {
    const previousPage = $workspaceView.selectedPageNumber;
    const previousQuestionId = $workspaceView.selectedQuestionId;
    workspaceState = next;

    if (resetQuestions) {
      questionDrafts = next.questions.map(toQuestionEdit);
    }
    if (!preserveProjectConfig) {
      projectConfigDraft = structuredClone(next.projectConfig ?? {
        projectId: next.project.projectId,
        displayName: next.project.displayName,
        subject: next.project.subject,
        courseCode: next.project.courseCode,
        lmsCourseId: next.project.lmsCourseId,
        lmsAssignmentId: null,
        redactionRequired: true,
        instructorProfile: structuredClone(defaultAppSettings.instructorProfile),
        traceRefs: { setupJobId: null, batchAnalyzeJobId: null, batchRubricJobId: null, intakeJobId: null },
        createdAt: next.project.createdAt,
        updatedAt: next.project.updatedAt
      });
    }

    workspaceView.syncPageSelection(
      next.templatePreviewArtifacts.map((page) => page.pageNumber),
      previousPage
    );
    workspaceView.syncQuestionSelection(
      questionDrafts.map((question) => question.questionId),
      previousQuestionId
    );

    shellState.update((current) => ({
      ...current,
      currentProject: next.project
    }));

    if (
      $workspaceView.activeWorkflowStep === 'moderation' ||
      $workspaceView.activeWorkflowStep === 'exportResults'
    ) {
      void refreshStudentDisplayNames(next);
    } else {
      clearStudentDisplayNames();
    }
  }

  function setStudentReviewSaveBusy(key: string, busy: boolean): void {
    studentReviewSaveBusy = setStudentReviewSaveBusyState(studentReviewSaveBusy, key, busy);
  }

  function markDeferredStudentWorkflowContinuation(next: ExamWorkspaceState): void {
    if (!hasRunnableStudentWorkflowRows(next)) {
      return;
    }
    deferredStudentWorkflowContinuation = true;
    if (busyAction !== 'studentWorkflow') {
      void runDeferredStudentWorkflowContinuation();
    }
  }

  async function runDeferredStudentWorkflowContinuation(): Promise<void> {
    if (
      !deferredStudentWorkflowContinuation ||
      studentWorkflowContinuationQueued ||
      busyAction !== null
    ) {
      return;
    }
    if (!hasRunnableStudentWorkflowRows(workspaceState)) {
      deferredStudentWorkflowContinuation = false;
      return;
    }
    deferredStudentWorkflowContinuation = false;
    studentWorkflowContinuationQueued = true;
    try {
      await handleBeginStudentWorkflow();
    } finally {
      studentWorkflowContinuationQueued = false;
      if (deferredStudentWorkflowContinuation) {
        void runDeferredStudentWorkflowContinuation();
      }
    }
  }

  function handleRosterCacheRuntimeEvent(event: RuntimeJobEvent): void {
    if (
      event.eventType === 'lms_roster_cache_updated' &&
      hasDesktopHost &&
      workspaceState &&
      ($workspaceView.activeWorkflowStep === 'moderation' ||
        $workspaceView.activeWorkflowStep === 'exportResults')
    ) {
      void refreshStudentDisplayNames(workspaceState);
    }
  }

  function handleResultsLmsUploadRuntimeEvent(event: RuntimeJobEvent): void {
    applyResultsUploadRuntimeEvent(event);
  }

  function matchesCreateProjectEvent(
    event: RuntimeJobEvent,
    createJobId: string | null
  ): boolean {
    if (event.commandName !== 'create_project') {
      return false;
    }
    if (createJobId === null) {
      return true;
    }
    return event.jobId === createJobId;
  }

  async function handleCreateProject() {
    busyAction = 'create';
    actionError = null;
    runtimeResult = null;
    let createJobId: string | null = null;
    let unsub = () => {};
    unsub = onJobCompletion({
      onFinished: (event) => {
        if (!matchesCreateProjectEvent(event, createJobId)) {
          return;
        }
        unsub();
        busyAction = null;
        let shell: ShellState;
        try {
          shell = hostWorkflowData<ShellState>(event, 'shell');
        } catch (error) {
          actionError = String(error);
          return;
        }
        if (shell.currentProject) {
          shellState.set(shell);
          saveRecentProject(
            shell.currentProject.projectPath,
            shell.currentProject.displayName,
            shell.currentProject.courseCode ?? null
          );
        }
        showCreateForm = false;
        createInput = {
          displayName: '',
          subject: '',
          courseCode: '',
          lmsCourseId: null,
          projectRoot: null,
          templatePdfPath: '',
          instructorProfile: undefined
        };
        workspaceView.reset();
        void loadWorkspaceState(true);
      },
      onFailed: (event) => {
        if (!matchesCreateProjectEvent(event, createJobId)) {
          return;
        }
        unsub();
        busyAction = null;
        actionError = hostWorkflowErrorMessage(event);
      }
    });
    try {
      createJobId = await createProject(
        {
          displayName: createInput.displayName,
          subject: createInput.subject?.trim() || null,
          courseCode: createInput.courseCode?.trim() || null,
          lmsCourseId: isCanvasLmsReady($appSettings) ? createInput.lmsCourseId : null,
          projectRoot: $appSettings.projectsDirectory,
          templatePdfPath: createInput.templatePdfPath,
          instructorProfile: structuredClone($appSettings.instructorProfile)
        },
        structuredClone($appSettings)
      );
    } catch (error) {
      unsub();
      busyAction = null;
      actionError = String(error);
      jobProgress.set(null);
    }
  }

  async function handleOpenProject() {
    busyAction = 'open';
    actionError = null;
    runtimeResult = null;
    try {
      let defaultRoot: string | undefined;
      try {
        defaultRoot = $appSettings.projectsDirectory || defaultProjectsRoot || await getDefaultProjectsRoot() || undefined;
      } catch {
        // Projects root may not be resolvable on this machine; open dialog without a default path.
      }
      const selection = await open({
        directory: true,
        multiple: false,
        title: 'Open ScriptScore Project',
        defaultPath: defaultRoot
      });
      if (typeof selection === 'string') {
        const shell = await openProject(selection, structuredClone($appSettings));
        shellState.set(shell);
        if (shell.currentProject) {
          saveRecentProject(shell.currentProject.projectPath, shell.currentProject.displayName, shell.currentProject.courseCode);
        }
        workspaceView.reset();
        await loadWorkspaceState(true);
      }
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  async function handleOpenRecentProject(projectPath: string) {
    busyAction = 'open';
    actionError = null;
    runtimeResult = null;
    try {
      const shell = await openProject(projectPath, structuredClone($appSettings));
      shellState.set(shell);
      if (shell.currentProject) {
        saveRecentProject(shell.currentProject.projectPath, shell.currentProject.displayName, shell.currentProject.courseCode);
      }
      workspaceView.reset();
      await loadWorkspaceState(true);
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  async function handleCloseProject() {
    busyAction = 'close';
    actionError = null;
    runtimeResult = null;
    try {
      shellState.set(await closeCurrentProject());
      workspaceState = null;
      questionDrafts = [];
      clearStudentDisplayNames();
      workspaceView.reset();
      showCreateForm = false;
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  async function chooseTemplatePdfForCreate() {
    actionError = null;
    const selection = await open({
      multiple: false,
      title: 'Choose Template PDF',
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    });
    if (typeof selection === 'string') {
      createInput = { ...createInput, templatePdfPath: selection };
    }
  }

  async function chooseProjectsDirectory() {
    actionError = null;
    const selection = await open({
      directory: true,
      multiple: false,
      title: 'Choose ScriptScore Projects Folder',
      defaultPath:
        settingsDraft.projectsDirectory ??
        $appSettings.projectsDirectory ??
        defaultProjectsRoot ??
        undefined
    });
    if (typeof selection === 'string') {
      handleSettingsChange({
        ...settingsDraft,
        projectsDirectory: selection
      });
    }
  }

  async function choosePiiPaddleModelDirectory() {
    actionError = null;
    const selection = await open({
      directory: true,
      multiple: false,
      title: 'Choose PII Paddle Model Folder',
      defaultPath:
        settingsDraft.piiPaddleModelDir ??
        $appSettings.piiPaddleModelDir ??
        undefined
    });
    if (typeof selection === 'string') {
      handleSettingsChange({
        ...settingsDraft,
        piiPaddleModelDir: selection
      });
    }
  }

  function clearProjectsDirectory() {
    handleSettingsChange({
      ...settingsDraft,
      projectsDirectory: null
    });
  }

  function clearPiiPaddleModelDirectory() {
    handleSettingsChange({
      ...settingsDraft,
      piiPaddleModelDir: null
    });
  }

  async function runSetupWizardFromSettings() {
    actionError = null;
    runtimeResult = null;
    showLandingSettings = false;
    if ($shellState.currentProject) {
      await handleCloseProject();
      if (actionError) {
        return;
      }
    }
    forceOnboardingOpen = true;
  }

  async function handleReplaceTemplatePdf() {
    try {
      const selection = await open({
        multiple: false,
        title: 'Replace Template PDF',
        filters: [{ name: 'PDF', extensions: ['pdf'] }]
      });
      if (typeof selection === 'string') {
        await runTrackedWorkspaceCommand({
          busy: 'replaceTemplate',
          start: () => replaceTemplatePdf(selection),
          resetQuestions: true,
          onFinished: () => {
            selectTemplateSubstep('setup');
          }
        });
      }
    } catch (error) {
      actionError = String(error);
    }
  }

  function templateExportDefaultPath(): string {
    const name = slugifyFilename(workspaceState?.project.displayName ?? 'template') || 'template';
    return `${name}-template.pdf`;
  }

  function slugifyFilename(value: string): string {
    const chars: string[] = [];
    let previousWasDash = false;
    for (const character of value.trim().toLowerCase()) {
      const code = character.charCodeAt(0);
      const isAsciiLetter = code >= 97 && code <= 122;
      const isDigit = code >= 48 && code <= 57;
      if (isAsciiLetter || isDigit) {
        chars.push(character);
        previousWasDash = false;
      } else if (!previousWasDash && chars.length > 0) {
        chars.push('-');
        previousWasDash = true;
      }
    }
    if (chars[chars.length - 1] === '-') {
      chars.pop();
    }
    return chars.join('');
  }

  async function handleExportTemplatePdf() {
    try {
      const destinationPath = await save({
        title: 'Export Template PDF',
        defaultPath: templateExportDefaultPath(),
        filters: [{ name: 'PDF', extensions: ['pdf'] }]
      });
      if (typeof destinationPath !== 'string') {
        return;
      }
      busyAction = 'exportTemplate';
      alignmentStampJobActive = templateExportNeedsAlignmentStamp();
      actionError = null;
      const next = await runTrackedWorkspaceCommand({
        busy: 'exportTemplate',
        start: () => exportStampedTemplatePdf(destinationPath),
        resetQuestions: true
      });
      if (next) {
        notifications.pushSuccess('Template PDF exported');
      }
    } catch (error) {
      actionError = String(error);
    } finally {
      alignmentStampJobActive = false;
    }
  }

  function templateExportNeedsAlignmentStamp(): boolean {
    const status = workspaceState?.arucoStatus;
    return status?.state === 'not_detected' && (status.totalMarkerCount ?? 0) <= 0;
  }

  async function handleRuntimeCheck() {
    busyAction = 'smoke';
    actionError = null;
    runtimeResult = null;
    try {
      runtimeResult = await runSmokePing();
      await refreshShellState();
    } catch (error) {
      actionError = String(error);
      await refreshShellState();
    } finally {
      busyAction = null;
    }
  }

  async function handleSaveRegions(regions: TemplateRedactionRegionInput[]) {
    busyAction = 'saveRegions';
    actionError = null;
    try {
      const next = await saveRedactionRegions(
        [...regions]
          .sort((left, right) => left.pageNumber - right.pageNumber)
          .map((region) => ({
            regionId: region.regionId,
            pageNumber: region.pageNumber,
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height
          }))
      );
      applyWorkspaceState(next, false);
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  async function handlePageRegionChange(regionsForSelectedPage: TemplateRedactionRegionInput[]) {
    if (!workspaceState) {
      return;
    }
    const nextByExistingId = new Map(
      regionsForSelectedPage
        .filter((region) => region.regionId)
        .map((region) => [region.regionId, region])
    );
    const nextRegions: TemplateRedactionRegionInput[] = [];
    const consumedIds = new Set<string>();
    for (const existing of [...workspaceState.redactionRegions].sort(
      (left, right) => left.sortOrder - right.sortOrder
    )) {
      if (existing.pageNumber !== $workspaceView.selectedPageNumber) {
        nextRegions.push({
          regionId: existing.regionId,
          pageNumber: existing.pageNumber,
          x: existing.x,
          y: existing.y,
          width: existing.width,
          height: existing.height
        });
        continue;
      }
      const replacement = nextByExistingId.get(existing.regionId);
      if (replacement) {
        nextRegions.push(replacement);
        consumedIds.add(existing.regionId);
      }
    }
    nextRegions.push(
      ...regionsForSelectedPage.filter(
        (region) => !region.regionId || !consumedIds.has(region.regionId)
      )
    );
    await handleSaveRegions(nextRegions);
  }

  function updateQuestionDraft(
    questionId: string,
    patch: Partial<Omit<QuestionEdit, 'questionId'>>
  ) {
    questionDrafts = questionDrafts.map((draft) =>
      draft.questionId === questionId ? { ...draft, ...patch } : draft
    );
  }

  function discardQuestionDraft(questionId: string) {
    const question = workspaceState?.questions.find((item) => item.questionId === questionId);
    if (!question) {
      return;
    }
    questionDrafts = questionDrafts.map((draft) =>
      draft.questionId === questionId ? toQuestionEdit(question) : draft
    );
  }

  function withQuestionRubricEditImpact(
    edit: QuestionEdit,
    approvedEditImpact: ApprovedEditImpact | null
  ): QuestionEdit {
    const question = workspaceState?.questions.find((item) => item.questionId === edit.questionId);
    if (!question || !rubricIsApproved(question.rubric)) {
      return edit;
    }
    const currentContext = normalizeQuestionContext(question.analysis?.questionContext ?? undefined);
    const basisChanged =
      question.text !== edit.text ||
      question.maxPoints !== edit.maxPoints ||
      currentContext !== normalizeQuestionContext(edit.questionContext);
    if (!basisChanged) {
      return edit;
    }
    if (question.maxPoints !== edit.maxPoints) {
      return { ...edit, rubricEditImpact: 'grading' };
    }
    return { ...edit, rubricEditImpact: approvedEditImpact ?? 'grading' };
  }

  function rubricEditImpactForSave(
    questionId: string,
    criteria: RubricCriterion[],
    approvedEditImpact: ApprovedEditImpact | null
  ): ApprovedEditImpact | null {
    const question = workspaceState?.questions.find((item) => item.questionId === questionId);
    const currentCriteria = question?.rubric?.criteria ?? [];
    if (!rubricIsApproved(question?.rubric)) {
      return null;
    }
    if (approvedEditImpact === 'grading') {
      return 'grading';
    }
    if (JSON.stringify(currentCriteria) === JSON.stringify(criteria)) {
      return null;
    }
    return approvedEditImpact ?? 'grading';
  }

  function selectTemplateSubstep(step: TemplateSetupSubstep) {
    workspaceView.setWorkflowStep('templateSetup');
    workspaceView.setTemplateSetupSubstep(step);
    if (step === 'review' && !$workspaceView.selectedQuestionId) {
      workspaceView.syncQuestionSelection(
        orderedQuestionDrafts.map((question) => question.questionId),
        null
      );
    }
  }

  async function handleSaveProjectSetup() {
    if (!projectConfigDraft) {
      return;
    }
    busyAction = 'saveSetup';
    actionError = null;
    try {
      const assignmentId = projectConfigDraft.lmsAssignmentId?.trim() || null;
      const existingAssignmentId = workspaceState?.projectConfig?.lmsAssignmentId?.trim() || null;
      const courseId = projectConfigDraft.lmsCourseId?.trim() || null;
      const existingCourseId = workspaceState?.projectConfig?.lmsCourseId?.trim() || null;
      const shouldSaveAssignmentTarget =
        assignmentId !== null ||
        existingAssignmentId !== null ||
        courseId !== existingCourseId;
      const projectConfigToSave = shouldSaveAssignmentTarget
        ? { ...projectConfigDraft, lmsAssignmentId: null }
        : projectConfigDraft;
      let next = await saveProjectConfig(projectConfigToSave, structuredClone($appSettings));
      if (shouldSaveAssignmentTarget) {
        next = await saveResultsLmsAssignment({ assignmentId });
      }
      applyWorkspaceState(next, false);
      saveRecentProject(
        next.project.projectPath,
        next.project.displayName,
        next.project.courseCode
      );
      notifications.pushSuccess('Template setup saved');
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  function handleSettingsChange(next: AppSettings) {
    const committed = structuredClone(next);
    settingsDraft = committed;
    appSettings.save(committed);
  }

  async function runTrackedWorkspaceCommand(options: {
    busy: BusyAction;
    start: () => Promise<string>;
    resetQuestions?: boolean;
    onFinished?: (next: ExamWorkspaceState) => void;
  }): Promise<ExamWorkspaceState | null> {
    busyAction = options.busy;
    actionError = null;
    let stopListening = () => {};
    return await new Promise<ExamWorkspaceState | null>((resolve) => {
      void (async () => {
        try {
          const jobId = await options.start();
          activeWorkspaceJobId = jobId;
          stopListening = onJobCompletion(
            {
              onFinished: (event) => {
                stopListening();
                busyAction = null;
                stopWorkflowBusy = false;
                if (activeWorkspaceJobId === event.jobId) {
                  activeWorkspaceJobId = null;
                }
                let next: ExamWorkspaceState;
                try {
                  next = hostWorkflowData<ExamWorkspaceState>(event, 'workspace');
                } catch (error) {
                  actionError = String(error);
                  resolve(null);
                  return;
                }
                applyWorkspaceState(next, options.resetQuestions ?? false);
                options.onFinished?.(next);
                if (options.busy === 'studentWorkflow') {
                  void runDeferredStudentWorkflowContinuation();
                }
                resolve(next);
              },
              onFailed: (event) => {
                stopListening();
                busyAction = null;
                stopWorkflowBusy = false;
                if (activeWorkspaceJobId === event.jobId) {
                  activeWorkspaceJobId = null;
                }
                actionError = event.eventType === 'job_cancelled'
                  ? null
                  : hostWorkflowErrorMessage(event);
                resolve(null);
              }
            },
            jobId
          );
        } catch (error) {
          stopListening();
          busyAction = null;
          stopWorkflowBusy = false;
          activeWorkspaceJobId = null;
          actionError = String(error);
          resolve(null);
        }
      })();
    });
  }

  async function startTrackedWorkspaceCommand(options: {
    busy: BusyAction;
    start: () => Promise<string>;
    resetQuestions?: boolean;
    onFinished?: (next: ExamWorkspaceState) => void;
  }): Promise<void> {
    busyAction = options.busy;
    actionError = null;
    let stopListening = () => {};
    try {
      const jobId = await options.start();
      activeWorkspaceJobId = jobId;
      stopListening = onJobCompletion(
        {
          onFinished: (event) => {
            stopListening();
            busyAction = null;
            stopWorkflowBusy = false;
            if (activeWorkspaceJobId === event.jobId) {
              activeWorkspaceJobId = null;
            }
            let next: ExamWorkspaceState;
            try {
              next = hostWorkflowData<ExamWorkspaceState>(event, 'workspace');
            } catch (error) {
              actionError = String(error);
              return;
            }
            applyWorkspaceState(next, options.resetQuestions ?? false);
            options.onFinished?.(next);
            if (options.busy === 'studentWorkflow') {
              void runDeferredStudentWorkflowContinuation();
            }
          },
          onFailed: (event) => {
            stopListening();
            busyAction = null;
            stopWorkflowBusy = false;
            if (activeWorkspaceJobId === event.jobId) {
              activeWorkspaceJobId = null;
            }
            actionError = event.eventType === 'job_cancelled'
              ? null
              : hostWorkflowErrorMessage(event);
          }
        },
        jobId
      );
    } catch (error) {
      stopListening();
      busyAction = null;
      stopWorkflowBusy = false;
      activeWorkspaceJobId = null;
      actionError = String(error);
      throw error;
    }
  }

  async function handleGenerateRubric(questionId: string) {
    const current = workspaceState?.questions.find((question) => question.questionId === questionId);
    if (
      rubricIsApproved(current?.rubric) &&
      !(await confirmOperatorAction(
        'Generating rubric criteria for an approved rubric will require reapproval and may make existing grading stale. Continue?',
        'Generate rubric'
      ))
    ) {
      return;
    }
    const hasCriteria = (current?.rubric?.criteria.length ?? 0) > 0;
    const replaceExisting =
      hasCriteria
        ? await confirmOperatorAction(
            'Replace the existing rubric criteria? Choose Cancel to append the generated criteria instead.',
            'Generate rubric'
          )
        : true;
    await runTrackedWorkspaceCommand({
      busy: 'generateRubric',
      start: () =>
        generateQuestionRubric(questionId, replaceExisting, structuredClone($appSettings)),
      onFinished: async () => {
        try {
          const fresh = await getExamWorkspaceState();
          applyWorkspaceState(fresh, false);
        } catch {
          /* keep completion payload if refresh fails */
        }
      }
    });
  }

  async function handleReAnalyzeQuestion(questionId: string) {
    if (hasUnsavedQuestionEdits) {
      actionError = 'Save question edits before re-analyzing.';
      return;
    }
    if (activeAnalysisJobByQuestion[questionId]) {
      return;
    }
    targetedAnalysisInFlightCount += 1;
    setQuestionAnalysisJobState(questionId, 'running');
    actionError = null;
    let stopListening = () => {};
    const clearTargetedState = () => {
      stopListening();
      targetedAnalysisInFlightCount = Math.max(0, targetedAnalysisInFlightCount - 1);
      clearQuestionAnalysisJobState(questionId);
    };
    try {
      const jobId = await reanalyzeQuestion(questionId, structuredClone($appSettings));
      stopListening = onJobCompletion(
        {
          onFinished: async () => {
            clearTargetedState();
            try {
              const next = await getExamWorkspaceState();
              applyWorkspaceState(next, false);
            } catch (error) {
              actionError = String(error);
            }
          },
          onFailed: (event) => {
            clearTargetedState();
            actionError = event.eventType === 'job_cancelled'
              ? null
              : hostWorkflowErrorMessage(event);
          }
        },
        jobId
      );
    } catch (error) {
      clearTargetedState();
      actionError = String(error);
    }
  }

  async function handleSaveRubric(
    questionId: string,
    criteria: RubricCriterion[],
    approve: boolean
  ) {
    const rubricEditImpact = approve ? null : rubricEditImpactForSave(questionId, criteria, null);
    busyAction = 'saveRubric';
    actionError = null;
    try {
      const next = await saveRubricUpdate({
        questionId,
        criteria,
        approve,
        ...(rubricEditImpact ? { rubricEditImpact } : {})
      });
      applyWorkspaceState(next, false);
      notifications.pushSuccess(approve ? 'Approved rubric saved' : 'Rubric saved');
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  async function handleSaveReviewChanges(
    questionId: string,
    criteria: RubricCriterion[],
    saveQuestions: boolean,
    saveRubric: boolean,
    approvedEditImpact: ApprovedEditImpact | null = null
  ) {
    busyAction = 'saveQuestions';
    actionError = null;
    try {
      if (saveQuestions) {
        const edits: QuestionEdit[] = [];
        for (const draft of questionDrafts) {
          edits.push(withQuestionRubricEditImpact(draft, approvedEditImpact));
        }
        const next = await saveQuestionEdits(edits);
        applyWorkspaceState(next, true);
      }
      if (saveRubric) {
        const rubricEditImpact = rubricEditImpactForSave(questionId, criteria, approvedEditImpact);
        busyAction = 'saveRubric';
        const next = await saveRubricUpdate({
          questionId,
          criteria,
          approve: false,
          ...(rubricEditImpact ? { rubricEditImpact } : {})
        });
        applyWorkspaceState(next, false);
      }
      notifications.pushSuccess('Review changes saved');
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  /** Resolve the selected LMS student to the persisted project roster, then start the tracked `run_student_intake` job. */
  async function handleRunStudentIntake(
    payload: StudentIntakeFinalizePayload,
    hooks?: { onBindingPersisted?: () => void }
  ): Promise<StudentIntakeFinalizeResult | null> {
    const courseId = payload.courseId?.trim() || null;
    const canvasUserId = payload.canvasUserId?.trim() || null;
    const localStudentName = payload.localStudentName?.trim() || null;
    const binding =
      courseId && canvasUserId
        ? await resolveLmsStudentRef(courseId, canvasUserId)
        : null;
    const studentRef = binding?.studentRef ?? nextLocalStudentRef(workspaceState);
    hooks?.onBindingPersisted?.();
    intakeDrafts = [
      {
        rawPdfPath: payload.rawPdfPath,
        studentRef,
        localStudentName,
        desiredPageOrder: [...payload.desiredPageOrder],
        redactionRegionsPx: payload.redactionRegionsPx.map((region) => ({ ...region })),
        rasterSizesByPage: Object.fromEntries(
          Object.entries(payload.rasterSizesByPage).map(([pageNumber, size]) => [
            Number(pageNumber),
            { ...size }
          ])
        )
      }
    ];
    const nextWorkspaceState = await runTrackedWorkspaceCommand({
      busy: 'studentIntake',
      start: async () =>
        runStudentIntake(
          intakeDrafts
            .filter((draft) => draft.rawPdfPath.trim() && draft.studentRef.trim())
            .map((draft) => ({
              rawPdfPath: draft.rawPdfPath.trim(),
              studentRef: draft.studentRef.trim(),
              localStudentName: draft.localStudentName?.trim() || null,
              desiredPageOrder: [...(draft.desiredPageOrder ?? [])],
              redactionRegionsPx: (draft.redactionRegionsPx ?? []).map((region) => ({ ...region })),
              rasterSizesByPage: Object.fromEntries(
                Object.entries(draft.rasterSizesByPage ?? {}).map(([pageNumber, size]) => [
                  Number(pageNumber),
                  { ...size }
                ])
              )
            }))
        ),
      onFinished: () => {
        intakeDrafts = [];
      }
    });
    if (!nextWorkspaceState) {
      intakeDrafts = [];
      if (typeof actionError === 'string' && actionError.trim().length > 0) {
        throw actionError;
      }
      if (typeof visibleRuntimeError === 'string' && visibleRuntimeError.trim().length > 0) {
        throw visibleRuntimeError;
      }
      return null;
    }
    return {
      workspaceState: nextWorkspaceState,
      studentRef,
      bindingTokenHex: binding?.bindingTokenHex ?? null
    };
  }

  function nextLocalStudentRef(state: ExamWorkspaceState | null): string {
    const refs = [
      ...(state?.studentIntake?.items ?? []).map((item) => item.studentRef),
      ...(state?.studentWorkflow?.submissions ?? []).map((submission) => submission.studentRef)
    ];
    const max = refs.reduce((current, ref) => {
      const match = /^student_(\d+)$/.exec(ref.trim());
      return match ? Math.max(current, Number(match[1])) : current;
    }, 0);
    return `student_${max + 1}`;
  }

  async function handleBeginStudentWorkflow() {
    await runTrackedWorkspaceCommand({
      busy: 'studentWorkflow',
      start: () => beginStudentWorkflow(structuredClone($appSettings)),
      resetQuestions: false
    });
  }

  async function handleDeleteStudentSubmission(studentRef: string) {
    busyAction = 'studentIntake';
    actionError = null;
    try {
      const next = await deleteStudentSubmission({ studentRef });
      applyWorkspaceState(next, false);
      notifications.pushSuccess('Student submission deleted');
    } catch (error) {
      actionError = String(error);
      throw error;
    } finally {
      busyAction = null;
    }
  }

  async function handleStopStudentWorkflow() {
    if (busyAction !== 'studentWorkflow' || stopWorkflowBusy) {
      return;
    }
    stopWorkflowBusy = true;
    actionError = null;
    try {
      const nextShell = await cancelActiveJob(null);
      shellState.set(nextShell);
    } catch (error) {
      stopWorkflowBusy = false;
      actionError = String(error);
    }
  }

  async function handleConfirmStudentAlignment(
    studentRef: string,
    pages: import('$lib/types').StudentWorkflowAlignmentPage[]
  ) {
    if (busyAction === 'studentWorkflow') {
      const key = studentReviewSaveKey('alignment', studentRef);
      setStudentReviewSaveBusy(key, true);
      actionError = null;
      try {
        const next = await saveStudentAlignmentReview(studentRef, pages);
        applyWorkspaceState(next, false);
        markDeferredStudentWorkflowContinuation(next);
      } catch (error) {
        actionError = String(error);
        throw error;
      } finally {
        setStudentReviewSaveBusy(key, false);
      }
      return;
    }
    await startTrackedWorkspaceCommand({
      busy: 'studentWorkflow',
      start: () => confirmStudentAlignment(studentRef, pages, structuredClone($appSettings)),
      resetQuestions: false
    });
  }

  async function handleConfirmStudentParseReview(
    studentRef: string,
    questionId: string,
    correctedText: string
  ) {
    if (busyAction === 'studentWorkflow') {
      const key = studentReviewSaveKey('parse', studentRef, questionId);
      setStudentReviewSaveBusy(key, true);
      actionError = null;
      try {
        const next = await saveStudentParseReview(studentRef, questionId, correctedText);
        applyWorkspaceState(next, false);
        markDeferredStudentWorkflowContinuation(next);
      } catch (error) {
        actionError = String(error);
        throw error;
      } finally {
        setStudentReviewSaveBusy(key, false);
      }
      return;
    }
    await runTrackedWorkspaceCommand({
      busy: 'studentWorkflow',
      start: () =>
        confirmStudentParseReview(
          studentRef,
          questionId,
          correctedText,
          structuredClone($appSettings)
        ),
      resetQuestions: false
    });
  }

  async function handleConfirmStudentDetectReview(
    studentRef: string,
    resolutions: import('$lib/types').StudentWorkflowDetectReviewResolutionInput[]
  ) {
    if (busyAction === 'studentWorkflow') {
      const key = studentReviewSaveKey('detect', studentRef);
      setStudentReviewSaveBusy(key, true);
      actionError = null;
      try {
        const next = await saveStudentDetectReview(studentRef, resolutions);
        applyWorkspaceState(next, false);
        markDeferredStudentWorkflowContinuation(next);
      } catch (error) {
        actionError = String(error);
        throw error;
      } finally {
        setStudentReviewSaveBusy(key, false);
      }
      return;
    }
    await startTrackedWorkspaceCommand({
      busy: 'studentWorkflow',
      start: () => confirmStudentDetectReview(studentRef, resolutions, structuredClone($appSettings)),
      resetQuestions: false
    });
  }

  async function handleSaveCriterionScore(
    studentRef: string,
    questionId: string,
    criterionIndex: number,
    pointsAwarded: number
  ) {
    const key = studentReviewSaveKey('criterion', studentRef, questionId);
    if (studentReviewSaveBusy[key]) {
      return;
    }
    setStudentReviewSaveBusy(key, true);
    actionError = null;
    try {
      const next = await saveCriterionScore({
        studentRef,
        questionId,
        criterionIndex,
        pointsAwarded
      });
      applyWorkspaceState(next, false);
      if (busyAction === 'studentWorkflow') {
        markDeferredStudentWorkflowContinuation(next);
      }
    } catch (error) {
      actionError = String(error);
    } finally {
      setStudentReviewSaveBusy(key, false);
    }
  }

  async function handleSaveModeratedScore(
    studentRef: string,
    questionId: string,
    moderatedTotalPoints: number
  ) {
    actionError = null;
    try {
      const next = await saveModeratedScore({
        studentRef,
        questionId,
        moderatedTotalPoints
      });
      applyWorkspaceState(next, false);
    } catch (error) {
      actionError = String(error);
    }
  }

  async function handleSaveModeratedFeedback(
    studentRef: string,
    questionId: string,
    feedbackText: string
  ) {
    actionError = null;
    try {
      const next = await saveModeratedFeedback({
        studentRef,
        questionId,
        feedbackText
      });
      applyWorkspaceState(next, false);
    } catch (error) {
      actionError = String(error);
    }
  }

  async function handleSetModerationQuestionReviewed(
    questionId: string,
    reviewed: boolean
  ): Promise<boolean> {
    actionError = null;
    try {
      const next = await setModerationQuestionReviewed({
        questionId,
        reviewed
      });
      applyWorkspaceState(next, false);
      return true;
    } catch (error) {
      actionError = String(error);
      return false;
    }
  }

  async function handleSetSubmissionResultFinalized(
    studentRef: string,
    finalized: boolean
  ) {
    busyAction = 'resultsAssignment';
    actionError = null;
    try {
      const next = await setSubmissionResultFinalized({ studentRef, finalized });
      applyWorkspaceState(next, false);
    } catch (error) {
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  async function handleFinalizeReadyResults(studentRefs: string[]) {
    busyAction = 'resultsAssignment';
    actionError = null;
    try {
      const next = await finalizeReadyResults({ studentRefs });
      applyWorkspaceState(next, false);
      return true;
    } catch (error) {
      actionError = String(error);
      return false;
    } finally {
      busyAction = null;
    }
  }

  async function handleRunResultsLmsUpload(
    mode: 'dry_run' | 'live',
    studentRefs: string[]
  ) {
    const courseId =
      (workspaceState?.projectConfig?.lmsCourseId ?? workspaceState?.project.lmsCourseId ?? '').trim();
    if (!courseId) {
      actionError = 'This project is not linked to an LMS course. Finalized results stay local.';
      return false;
    }
    busyAction = 'resultsUpload';
    actionError = null;
    try {
      const response = await runResultsLmsUpload({ mode, studentRefs });
      applyWorkspaceState(response.workspace, false);
      notifications.pushSuccess(
        mode === 'dry_run' ? 'Dry run completed' : 'LMS upload completed'
      );
      return true;
    } catch (error) {
      resultsWorkspaceView.failActiveUploadBatch();
      actionError = String(error);
      return false;
    } finally {
      busyAction = null;
    }
  }

  async function handleRetryResultsLmsUpload(attemptId: string) {
    const courseId =
      (workspaceState?.projectConfig?.lmsCourseId ?? workspaceState?.project.lmsCourseId ?? '').trim();
    if (!courseId) {
      actionError = 'This project is not linked to an LMS course. Finalized results stay local.';
      return;
    }
    busyAction = 'resultsUpload';
    actionError = null;
    try {
      const response = await retryResultsLmsUpload({ attemptId });
      applyWorkspaceState(response.workspace, false);
      notifications.pushSuccess('Retry completed');
    } catch (error) {
      resultsWorkspaceView.failActiveUploadBatch();
      actionError = String(error);
    } finally {
      busyAction = null;
    }
  }

  async function handlePreviewResultsLmsReport(studentRef: string) {
    return await previewResultsLmsReport(studentRef);
  }

  async function handleRunResultsExport(format: ResultsExportFormat, studentRefs: string[]) {
    const extension = format === 'html_zip' ? 'zip' : 'csv';
    let destinationPath: string | null = null;
    try {
      destinationPath = await save({
        title: 'Export results',
        defaultPath: defaultResultsExportFileName(extension),
        filters: [
          {
            name: format === 'html_zip' ? 'ZIP archive' : 'CSV file',
            extensions: [extension]
          }
        ]
      });
    } catch (error) {
      actionError = String(error);
      return false;
    }
    if (!destinationPath) {
      return false;
    }

    busyAction = 'resultsUpload';
    actionError = null;
    try {
      const response = await runResultsExport({ format, studentRefs, destinationPath });
      notifications.pushSuccess(
        `Exported ${response.exportedCount} ${response.exportedCount === 1 ? 'result' : 'results'} to ${fileNameFromPath(response.destinationPath)}`
      );
      return true;
    } catch (error) {
      actionError = String(error);
      return false;
    } finally {
      busyAction = null;
    }
  }

  function defaultResultsExportFileName(extension: 'zip' | 'csv') {
    const projectName = workspaceState?.project.displayName?.trim() || 'results';
    return `${projectName.replace(/[^A-Za-z0-9._-]+/g, '_')}-results.${extension}`;
  }

  function fileNameFromPath(path: string) {
    return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  }
</script>

<svelte:head>
  <title>{documentTitle}</title>
</svelte:head>

{#if $shellState.currentProject && workspaceState}
  <div class="h-screen overflow-hidden bg-surface-shell text-foreground">
    <div class="grid h-full grid-cols-[4.5rem_minmax(0,1fr)]">
      <ProjectRail
        activeWorkflowStep={$workspaceView.activeWorkflowStep}
        attentionByStep={attentionByStep}
        hasDesktopHost={hasDesktopHost}
        busy={busyAction !== null}
        onOpenProject={handleOpenProject}
        onCloseProject={handleCloseProject}
        onSelectTemplateSetupSubstep={selectTemplateSubstep}
        onSelectWorkflowStep={(step) => {
          workspaceView.setWorkflowStep(step);
          if (step === 'templateSetup' && workspaceState) {
            const cfg = projectConfigDraft ?? workspaceState.projectConfig;
            const redactionRequired = cfg?.redactionRequired ?? true;
            const regionsCount = workspaceState.redactionRegions.length;
            if (isTemplateRedactionsReadyForReview(redactionRequired, regionsCount)) {
              workspaceView.setTemplateSetupSubstep('review');
            } else {
              workspaceView.setTemplateSetupSubstep('setup');
            }
          } else if (
            (step === 'moderation' || step === 'exportResults') &&
            workspaceState
          ) {
            void refreshStudentDisplayNames(workspaceState);
          } else {
            clearStudentDisplayNames();
          }
        }}
      />

      <div class="flex min-h-0 flex-col overflow-hidden">
        <ProjectTopBar
          appTitle={appTitle}
          workerStatus={$shellState.workerStatus}
          workerProgress={$jobProgress}
          workerActivity={$shellState.workerActivity ?? { activeJobs: [], pendingJobCount: 0 }}
          workflowStage={workspaceState.workflowStage ?? 'template_setup_not_started'}
          workflowLabel={workspaceState.workflowLabel ?? 'Template setup not started'}
          onOpenActiveTraces={openActiveTraceHistory}
        />

        <WorkspaceMessages
          actionError={actionError}
          runtimeError={visibleRuntimeError}
          failureMessage={workspaceState.failureMessage}
          warnings={workspaceState.warnings}
          runtimeResult={$workspaceView.activeWorkflowStep === 'settings' ? null : runtimeResult}
        />

        <main class="min-h-0 flex-1 overflow-hidden">
          {#if $workspaceView.activeWorkflowStep === 'templateSetup'}
            {#if $workspaceView.activeTemplateSetupSubstep === 'setup'}
              <SetupWorkspace
                {workspaceState}
                projectConfig={projectConfigDraft ?? workspaceState.projectConfig ?? {
                  projectId: workspaceState.project.projectId,
                  displayName: workspaceState.project.displayName,
                  subject: workspaceState.project.subject,
                  courseCode: workspaceState.project.courseCode,
                  lmsCourseId: workspaceState.project.lmsCourseId,
                  lmsAssignmentId: null,
                  redactionRequired: true,
                  instructorProfile: structuredClone(defaultAppSettings.instructorProfile),
                  traceRefs: { setupJobId: null, batchAnalyzeJobId: null, batchRubricJobId: null, intakeJobId: null },
                  createdAt: workspaceState.project.createdAt,
                  updatedAt: workspaceState.project.updatedAt
                }}
                selectedPageNumber={$workspaceView.selectedPageNumber}
                busy={
                  busyAction === 'saveSetup' ||
                  busyAction === 'saveRegions' ||
                  busyAction === 'replaceTemplate' ||
                  busyAction === 'exportTemplate'
                }
                alignmentMarksPending={alignmentStampJobActive}
                onSelectPage={(pageNumber) => {
                  workspaceView.setSelectedPageNumber(pageNumber);
                }}
                onReplaceTemplatePdf={handleReplaceTemplatePdf}
                onExportTemplatePdf={handleExportTemplatePdf}
                onRegionsChange={(regions) => void handlePageRegionChange(regions)}
                onConfirmContinue={async () => {
                  if (projectConfigDraft) {
                    await handleSaveProjectSetup();
                  }
                  selectTemplateSubstep('review');
                }}
                onSaveSetup={handleSaveProjectSetup}
                onDiscardChanges={() => {
                  if (workspaceState?.projectConfig) {
                    projectConfigDraft = structuredClone(workspaceState.projectConfig);
                  }
                }}
                debugRedactionToggle={$shellState.debugFeatures?.redactionToggle ?? false}
              />
            {:else}
              <ReviewWorkspace
                {workspaceState}
                {questionDrafts}
                selectedQuestionId={$workspaceView.selectedQuestionId}
                {busyAction}
                {hasUnsavedQuestionEdits}
                onSelectQuestion={(questionId) => {
                  workspaceView.setSelectedQuestionId(questionId);
                }}
                onUpdateQuestion={updateQuestionDraft}
                onSaveReviewChanges={handleSaveReviewChanges}
                onDiscardReviewChanges={discardQuestionDraft}
                onGenerateRubric={handleGenerateRubric}
                onSaveRubric={handleSaveRubric}
                onReAnalyze={handleReAnalyzeQuestion}
                {analysisInProgress}
                analysisJobByQuestion={activeAnalysisJobByQuestion}
              />
            {/if}
          {:else if $workspaceView.activeWorkflowStep === 'students'}
            <StudentWorkflowWorkspace
              lmsCourseId={workspaceState.projectConfig?.lmsCourseId ?? workspaceState.project.lmsCourseId}
              {workspaceState}
              {busyAction}
              reviewSaveBusy={studentReviewSaveBusy}
              prerequisitesMet={studentIntakePrerequisitesMet(workspaceState)}
              onFinalizeSubmission={async (
                finalize: StudentIntakeFinalizePayload,
                hooks?: { onBindingPersisted?: () => void }
              ) => await handleRunStudentIntake(finalize, hooks)}
              onBeginWorkflow={handleBeginStudentWorkflow}
              onStopWorkflow={handleStopStudentWorkflow}
              onDeleteSubmission={handleDeleteStudentSubmission}
              stopWorkflowBusy={stopWorkflowBusy}
              onConfirmAlignment={handleConfirmStudentAlignment}
              onConfirmDetectReview={handleConfirmStudentDetectReview}
              onConfirmParseReview={handleConfirmStudentParseReview}
              onSaveCriterionScore={handleSaveCriterionScore}
            />
          {:else if $workspaceView.activeWorkflowStep === 'moderation'}
            <ModerationWorkspace
              {workspaceState}
              studentDisplayNamesByRef={studentDisplayNamesByRef}
              busy={busyAction !== null}
              onSaveModeratedFeedback={handleSaveModeratedFeedback}
              onSaveModeratedScore={handleSaveModeratedScore}
              onSetQuestionReviewed={handleSetModerationQuestionReviewed}
            />
          {:else if $workspaceView.activeWorkflowStep === 'exportResults'}
            <ResultsWorkspace
              {workspaceState}
              studentDisplayNamesByRef={studentDisplayNamesByRef}
              busy={busyAction === 'resultsAssignment' || busyAction === 'resultsUpload'}
              onSetResultFinalized={handleSetSubmissionResultFinalized}
              onFinalizeReady={handleFinalizeReadyResults}
              onRunUpload={handleRunResultsLmsUpload}
              onRunExport={handleRunResultsExport}
              onRetryUpload={handleRetryResultsLmsUpload}
              onLoadReportPreview={handlePreviewResultsLmsReport}
            />
          {:else if $workspaceView.activeWorkflowStep === 'settings'}
            <SettingsWorkspace
              settings={settingsDraft}
              busy={busyAction === 'saveSetup' || busyAction === 'smoke'}
              smokeResult={runtimeResult}
              {hasDesktopHost}
              {resolvedProjectsDirectory}
              hasCurrentProject={Boolean(workspaceState?.project ?? $shellState.currentProject)}
              {visionModels}
              visionModelsBusy={visionModelsBusy}
              visionModelsError={visionModelsError}
              onSettingsChange={handleSettingsChange}
              onRuntimeCheck={handleRuntimeCheck}
              onOpenTraceHistory={openTraceHistory}
              onChooseProjectsDirectory={chooseProjectsDirectory}
              onClearProjectsDirectory={clearProjectsDirectory}
              onChoosePiiPaddleModelDirectory={choosePiiPaddleModelDirectory}
              onClearPiiPaddleModelDirectory={clearPiiPaddleModelDirectory}
              onRunSetupWizard={runSetupWizardFromSettings}
            />
          {:else}
            <div class="flex h-full items-center justify-center text-workspace-text-secondary">
              Select a workspace.
            </div>
          {/if}
        </main>
      </div>
    </div>
  </div>
{:else if $shellState.currentProject}
  <div class="flex h-screen items-center justify-center bg-surface-shell text-muted-foreground">
    <Surface variant="cardRaised" bordered radius="3xl" class="px-6 py-5 text-lg">
      Loading project workspace...
    </Surface>
  </div>
{:else if showLandingSettings}
  <div class="flex h-screen flex-col overflow-hidden bg-surface-shell text-foreground">
    <header class="sticky top-0 z-10 border-b border-border-default bg-surface-shell/95 backdrop-blur">
      <div class="flex w-full items-center justify-between gap-4 px-4 py-4">
        <DesktopButton
          onclick={() => {
            showLandingSettings = false;
            actionError = null;
          }}
        >
          Back
        </DesktopButton>
        <div class="text-base font-semibold text-workspace-text-primary">Settings</div>
        <div class="w-[4.5rem]" aria-hidden="true"></div>
      </div>
    </header>
    <WorkspaceMessages actionError={actionError} runtimeError={visibleRuntimeError} />
    <NotificationToasts />
    <div class="min-h-0 flex-1 overflow-hidden">
      <SettingsWorkspace
        settings={settingsDraft}
        busy={busyAction === 'saveSetup' || busyAction === 'smoke'}
        smokeResult={runtimeResult}
        {hasDesktopHost}
        {resolvedProjectsDirectory}
        hasCurrentProject={Boolean(workspaceState?.project ?? $shellState.currentProject)}
        {visionModels}
        visionModelsBusy={visionModelsBusy}
        visionModelsError={visionModelsError}
        onSettingsChange={handleSettingsChange}
        onRuntimeCheck={handleRuntimeCheck}
        onOpenTraceHistory={openTraceHistory}
        onChooseProjectsDirectory={chooseProjectsDirectory}
        onClearProjectsDirectory={clearProjectsDirectory}
        onChoosePiiPaddleModelDirectory={choosePiiPaddleModelDirectory}
        onClearPiiPaddleModelDirectory={clearPiiPaddleModelDirectory}
        onRunSetupWizard={runSetupWizardFromSettings}
      />
    </div>
  </div>
{:else}
    <NoProjectScreen
      {hasDesktopHost}
      {showCreateForm}
      {busyAction}
      {createInput}
      {actionError}
      {forceOnboardingOpen}
      {visionModels}
      visionModelsBusy={visionModelsBusy}
      recentProjects={recentProjects}
    onShowCreateForm={() => {
      showCreateForm = true;
      actionError = null;
      runtimeResult = null;
    }}
    onHideCreateForm={() => {
      showCreateForm = false;
    }}
    onOpenProject={handleOpenProject}
    onOpenRecentProject={handleOpenRecentProject}
    onChooseTemplatePdfForCreate={chooseTemplatePdfForCreate}
    onSubmitCreate={handleCreateProject}
    onCloseOnboarding={() => {
      forceOnboardingOpen = false;
    }}
    onOpenSettings={() => {
      showLandingSettings = true;
      forceOnboardingOpen = false;
      actionError = null;
      runtimeResult = null;
    }}
  />
{/if}

{#if traceHistoryOpen}
  <TraceHistoryDialog
    open={traceHistoryOpen}
    loadSummaries={listJobTraces}
    loadTrace={(jobId) => getJobTrace(jobId, null)}
    initialJobIds={traceHistoryInitialJobIds}
    initialStateFilter={traceHistoryInitialStateFilter}
    onClose={() => {
      traceHistoryOpen = false;
    }}
  />
{/if}

<ConfirmDialog
  open={pendingOperatorConfirmation !== null}
  title={pendingOperatorConfirmation?.title ?? 'Confirm action'}
  description={pendingOperatorConfirmation?.message ?? ''}
  confirmLabel={pendingOperatorConfirmation?.confirmLabel ?? 'Confirm'}
  cancelLabel={pendingOperatorConfirmation?.cancelLabel ?? 'Cancel'}
  destructive={pendingOperatorConfirmation?.destructive ?? false}
  onCancel={() => resolveOperatorConfirmation(false)}
  onConfirm={() => resolveOperatorConfirmation(true)}
/>
