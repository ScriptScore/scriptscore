<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import { appSettings } from '$lib/stores/appSettings';
  import type {
    ExamWorkspaceState,
    LmsRosterCacheSnapshot,
    LmsRosterRow,
    StudentIntakeFinalizePayload,
    StudentIntakeFinalizeResult,
    StudentIntakeSummary,
    StudentRosterRow,
    StudentWorkflowAlignmentPage,
    StudentWorkflowDetectReviewResolutionInput,
    StudentWorkflowSubmission,
    RuntimeJobEvent
  } from '$lib/types';
  import {
    computeLmsBindingToken,
    ensureLmsRosterPreload,
    getLmsRosterCacheState
  } from '$lib/desktop';
  import { onRuntimeJobEvent, shellState } from '$lib/stores/shell';
  import StudentIntakeWorkspace from '$lib/components/desktop/StudentIntakeWorkspace.svelte';
  import StudentWorkflowSidebar from '$lib/components/desktop/StudentWorkflowSidebar.svelte';
  import StudentWorkflowBoard from '$lib/components/desktop/StudentWorkflowBoard.svelte';
  import SubmissionPagesView from '$lib/components/desktop/SubmissionPagesView.svelte';
  import AlignmentReviewView from '$lib/components/desktop/AlignmentReviewView.svelte';
  import DetectReviewView from '$lib/components/desktop/DetectReviewView.svelte';
  import QuestionDetailView from '$lib/components/desktop/QuestionDetailView.svelte';
  import { ConfirmDialog, DesktopButton } from '$lib/components/desktop/ui';
  import {
    stageLabel,
    stageProgressTone,
    stageProgressValue,
    stageProgressRange,
    commandProgressRange,
    commandWorkflowStage,
    readCliProgressPercent,
    isSubstageBoundaryProgressPayload,
    workflowBarValueFromCommandProgress,
    stageViewForSubmission,
    dotClassForState,
    labelForState,
    studentRefSortKey,
    updateWorkflowCommandProgressState
  } from '$lib/components/desktop/student-workflow-helpers';

  export let workspaceState: ExamWorkspaceState;
  export let busyAction: BusyAction = null;
  export let reviewSaveBusy: Record<string, boolean> = {};
  export let prerequisitesMet = false;
  export let lmsCourseId: string | null = null;
  export let onFinalizeSubmission: ((
    payload: StudentIntakeFinalizePayload,
    hooks?: { onBindingPersisted?: () => void }
  ) => Promise<StudentIntakeFinalizeResult | null>) | null = null;
  export let onBeginWorkflow: (() => Promise<void>) | null = null;
  export let onStopWorkflow: (() => Promise<void>) | null = null;
  export let onRecoverWorkflow: (() => Promise<void>) | null = null;
  export let onDeleteSubmission:
    | ((studentRef: string, nextSelectedStudentRef: string | null) => Promise<void>)
    | null = null;
  export let onSaveStudentIntakePageOrder:
    | ((studentRef: string, examPagePaths: string[]) => Promise<void>)
    | null = null;
  export let stopWorkflowBusy = false;
  export let onConfirmAlignment:
    | ((studentRef: string, pages: StudentWorkflowAlignmentPage[]) => Promise<void>)
    | null = null;
  export let onConfirmDetectReview:
    | ((studentRef: string, resolutions: StudentWorkflowDetectReviewResolutionInput[]) => Promise<void>)
    | null = null;
  export let onConfirmParseReview:
    | ((studentRef: string, questionId: string, correctedText: string) => Promise<void>)
    | null = null;
  export let onSaveCriterionScore:
    | ((studentRef: string, questionId: string, criterionIndex: number, pointsAwarded: number) => Promise<void>)
    | null = null;

  type ShellMode = 'home' | 'intake';

  type RosterRowWithBinding = LmsRosterRow & { bindingTokenHex?: string | null };
  type WorkflowProgressEntry = {
    stage: string;
    progress: number;
  };
  type WorkflowCommandProgressEntry = {
    totalStages: number;
    currentStage: number;
  };
  type WorkflowScopedQuestionProgressEntry = {
    stage: string;
    activeQuestionId: string | null;
    completedQuestionIds: string[];
    totalQuestionCount: number;
  };

  const stageProgressRangeEpsilon = 0.01;
  const activeAutomationStages = new Set([
    'alignment',
    'canonicalize',
    'detect',
    'crop',
    'pii',
    'parse',
    'grading'
  ]);

  let shellMode: ShellMode = 'home';
  let selectedStudentRef: string | null = null;

  let rosterCacheState: LmsRosterCacheSnapshot = {
    status: 'idle',
    projectPath: null,
    lmsProvider: null,
    courseId: null,
    rows: [],
    lastError: null,
    idleReason: null
  };
  let rosterBusy = false;
  let rosterError: string | null = null;
  let rosterActionLabel = 'Retry';
  let rosterActionDisabled = false;
  let rosterVerificationError: string | null = null;
  let rosterRows: RosterRowWithBinding[] = [];
  let rosterBindingKeyInFlight: string | null = null;
  let lastRosterBindingKey = '';
  let lastRosterPreloadKey = '';

  let intakeSeedPaths: string[] = [];
  let intakeSeedVersion = 0;
  let workflowProgressByStudentRef = new Map<string, WorkflowProgressEntry>();
  /** Bumps when per-student CLI progress changes so `$:` blocks re-run (Map mutations are not tracked). */
  let workflowProgressRevision = 0;
  let workflowCommandProgressByJobId = new Map<string, WorkflowCommandProgressEntry>();
  let workflowScopedQuestionProgressByStudentRef = new Map<
    string,
    WorkflowScopedQuestionProgressEntry
  >();
  let workflowStudentRefByJobId = new Map<string, string>();
  let latestWorkflowStudentRefByStage = new Map<string, string>();
  let activeIntakeFilename: string | null = null;
  let pendingDeleteStudent: { studentRef: string; displayName: string } | null = null;

  function saveBusyKey(kind: string, ...parts: Array<string | number>): string {
    return [kind, ...parts].join(':');
  }

  $: resolvedLmsCourseId =
    (workspaceState?.projectConfig?.lmsCourseId ?? workspaceState?.project?.lmsCourseId ?? lmsCourseId ?? '').trim() ||
    null;
  $: hasLmsCourse = resolvedLmsCourseId !== null;
  $: lmsRosterEnabled = $appSettings.lmsProvider === 'canvas' && hasLmsCourse;
  $: rosterBusy = lmsRosterEnabled && rosterCacheState.status === 'loading';
  $: rosterError = lmsRosterEnabled
    ? rosterVerificationError ?? rosterCacheState.lastError ?? rosterCacheState.idleReason ?? null
    : null;
  $: rosterActionLabel =
    !lmsRosterEnabled
      ? 'Local'
      : rosterCacheState.status === 'loading'
      ? 'Loading…'
      : rosterCacheState.status === 'ready'
        ? 'Loaded'
        : 'Retry';
  $: rosterActionDisabled =
    !lmsRosterEnabled || rosterCacheState.status === 'loading' || rosterCacheState.status === 'ready';
  $: rosterBindingsLoading = rosterBindingKeyInFlight !== null;
  $: rosterPreloadKey = [
    workspaceState?.project?.projectPath ?? '',
    resolvedLmsCourseId ?? '',
    $appSettings.lmsProvider,
    $appSettings.lmsCanvasBaseUrl.trim(),
    ($appSettings.lmsCanvasApiKey ?? '').trim()
  ].join('|');

  $: studentRoster = workspaceState?.studentRoster ?? [];
  $: intakeItems = workspaceState?.studentIntake?.items ?? [];
  $: intakeItemsByStudentRef = new Map(
    intakeItems.map((item) => [item.studentRef, item] as const)
  );
  $: duplicateRosterBindingTokens = (() => {
    const counts = new Map<string, number>();
    for (const row of rosterRows) {
      const token = row.bindingTokenHex?.trim() || null;
      if (!token) continue;
      counts.set(token, (counts.get(token) ?? 0) + 1);
    }
    return new Set(
      [...counts.entries()]
        .filter(([, count]) => count > 1)
        .map(([token]) => token)
    );
  })();

  $: liveRosterByToken = new Map(
    rosterRows
      .map((row) => {
        const token = overlayRosterBindingToken(row);
        return token ? ([token, row] as const) : null;
      })
      .filter((entry): entry is readonly [string, RosterRowWithBinding] => entry !== null)
  );
  $: workflowSubmissions = workspaceState?.studentWorkflow?.submissions ?? [];
  $: workflowAutomationActive = workflowSubmissions.some((submission) =>
    activeAutomationStages.has(submission.stage)
  );
  $: persistedStudentWorkflowRunning =
    workspaceState?.studentWorkflow?.status === 'running' ||
    workspaceState?.workflowStage === 'student_workflow_running' ||
    workspaceState?.workflowStage === 'student_grading';
  $: desktopRuntimeHasActiveJob =
    (($shellState.workerActivity?.activeJobs.length ?? 0) > 0) ||
    (($shellState.workerActivity?.pendingJobCount ?? 0) > 0);
  $: studentWorkflowRecoveryAvailable =
    persistedStudentWorkflowRunning &&
    (busyAction === null || busyAction === 'studentWorkflowRecovery') &&
    !desktopRuntimeHasActiveJob;
  $: studentWorkflowRunning =
    busyAction === 'studentWorkflow' ||
    busyAction === 'studentWorkflowRecovery' ||
    persistedStudentWorkflowRunning ||
    (($shellState.workerStatus === 'busy' || $shellState.workerStatus === 'starting') &&
      workflowAutomationActive);
  $: workflowByStudentRef = new Map(
    workflowSubmissions.map((submission) => [submission.studentRef, submission] as const)
  );
  $: questionById = new Map(
    (workspaceState?.questions ?? []).map((question) => [question.questionId, question] as const)
  );
  $: studentRosterSorted = [...studentRoster].sort((a, b) =>
    studentRefSortKey(a.studentRef).localeCompare(studentRefSortKey(b.studentRef))
  );

  $: rosterEntries = studentRosterSorted.map((student) => {
    const liveRow = liveRosterByToken.get(student.bindingTokenHex) ?? null;
    const intakeItem = intakeItemsByStudentRef.get(student.studentRef) ?? null;
    const workflowSubmission = intakeItem
      ? workflowByStudentRef.get(intakeItem.studentRef) ?? null
      : null;
    const displayName = liveRow?.displayName ?? rosterDisplayNameFallback();
    return {
      student,
      liveRow,
      displayName,
      intakeItem,
      workflowSubmission
    };
  });
  $: localRosterEntries = intakeItems
    .slice()
    .sort((a, b) => studentRefSortKey(a.studentRef).localeCompare(studentRefSortKey(b.studentRef)))
    .map((intakeItem) => {
      const workflowSubmission = workflowByStudentRef.get(intakeItem.studentRef) ?? null;
      return {
        student: {
          studentRef: intakeItem.studentRef,
          bindingTokenHex: ''
        },
        liveRow: null,
        displayName: intakeItem.localDisplayName?.trim() || 'Unnamed student',
        intakeItem,
        workflowSubmission
      };
    });
  $: visibleRosterEntries = resolvedLmsCourseId ? rosterEntries : localRosterEntries;

  $: sidebarEntries = visibleRosterEntries.map((entry) => ({
    studentRef: entry.student.studentRef,
    displayName: entry.displayName,
    dotClass: dotClassForState(entry.intakeItem, entry.workflowSubmission),
    label: labelForState(entry.intakeItem, entry.workflowSubmission),
    debugLine: '',
    statusGroup: sidebarStatusGroup(entry.intakeItem, entry.workflowSubmission),
    progress: entry.workflowSubmission ? progressForSubmission(entry.workflowSubmission) : 0
  }));

  $: selectedRosterEntry =
    visibleRosterEntries.find((entry) => entry.student.studentRef === selectedStudentRef) ?? null;
  $: selectedIntakeItem = selectedRosterEntry?.intakeItem ?? null;
  $: selectedWorkflowSubmission = selectedRosterEntry?.workflowSubmission ?? null;
  $: selectedDisplayName = selectedRosterEntry?.displayName ?? '';
  $: selectedHasSubmission = selectedIntakeItem !== null || selectedWorkflowSubmission !== null;
  $: expectedTemplatePageCount = Math.max(
    0,
    ...((workspaceState?.templatePreviewArtifacts ?? []).map((page) => page.pageNumber))
  );
  $: selectedHasExtraPages =
    expectedTemplatePageCount > 0 &&
    (selectedIntakeItem?.examPagePaths?.filter((path) => path.trim().length > 0).length ?? 0) >
      expectedTemplatePageCount;

  $: attentionItems = workflowSubmissions.filter((submission) =>
    ['alignment_review', 'detect_review', 'parse_review', 'manual_grading', 'failed'].includes(submission.stage)
  );
  $: processingCount = workflowSubmissions.filter((submission) =>
      ['alignment', 'canonicalize', 'detect', 'crop', 'pii', 'parse', 'grading'].includes(
      submission.stage
    )
  ).length;
  $: gradedCount = workflowSubmissions.filter((submission) => submission.stage === 'graded').length;
  $: readyCount = workflowSubmissions.filter((submission) =>
    ['intake_ready', 'stopped'].includes(submission.stage)
  ).length;

  $: stageView = selectedStudentRef
    ? stageViewForSubmission(selectedWorkflowSubmission)
    : null;

  $: currentView = shellMode === 'intake'
    ? 'intake'
    : selectedStudentRef
      ? selectedWorkflowSubmission?.stage === 'failed' && selectedHasExtraPages
        ? 'submissionPages'
        : stageView ?? 'home'
      : 'home';
  $: compactReviewHeader =
    currentView === 'alignmentReview' || currentView === 'detectReview' || currentView === 'questionDetail';

  $: canonicalReadyRows = (() => {
    void workflowProgressRevision;
    return visibleRosterEntries
      .filter(
        (entry) =>
          !!entry.intakeItem &&
          (entry.intakeItem.examPagePaths?.length ?? 0) > 0 &&
          entry.intakeItem.ingestStatus !== 'failed'
      )
      .map((entry) => ({
        student: entry.student,
        displayName: entry.displayName,
        item: entry.intakeItem!,
        workflowSubmission: entry.workflowSubmission,
        stageText: entry.workflowSubmission
          ? stageLabel(entry.workflowSubmission.stage)
          : 'waiting',
        stageTitle: entry.workflowSubmission
          ? progressTitleForSubmission(entry.workflowSubmission)
          : undefined,
        stageProgress: entry.workflowSubmission
          ? progressForSubmission(entry.workflowSubmission)
          : 0,
        stageTone: entry.workflowSubmission
          ? stageProgressTone(entry.workflowSubmission.stage)
          : 'muted',
        stageComplete: entry.workflowSubmission?.stage === 'graded',
        stageActive: entry.workflowSubmission
          ? activeAutomationStages.has(entry.workflowSubmission.stage)
          : false
      }));
  })();

  $: selectedStageProgress = (() => {
    void workflowProgressRevision;
    return selectedWorkflowSubmission ? progressForSubmission(selectedWorkflowSubmission) : 0;
  })();

  function normalizedRosterBindingToken(row: RosterRowWithBinding): string | null {
    return row.bindingTokenHex?.trim() || null;
  }

  function overlayRosterBindingToken(row: RosterRowWithBinding): string | null {
    const token = normalizedRosterBindingToken(row);
    if (!token || duplicateRosterBindingTokens.has(token)) {
      return null;
    }
    return token;
  }

  function selectStudent(studentRef: string) {
    selectedStudentRef = studentRef;
    shellMode = 'home';
  }

  function sidebarStatusGroup(
    intakeItem: StudentIntakeSummary | null,
    workflowSubmission: StudentWorkflowSubmission | null
  ):
    | 'needsReview'
    | 'manual'
    | 'processing'
    | 'ready'
    | 'graded'
    | 'failed'
    | 'stopped'
    | 'noSubmission' {
    if (!intakeItem) {
      return 'noSubmission';
    }
    if (workflowSubmission?.stage === 'graded') {
      return 'graded';
    }
    if (workflowSubmission?.stage === 'failed') {
      return 'failed';
    }
    if (workflowSubmission?.stage === 'stopped') {
      return 'stopped';
    }
    if (workflowSubmission?.stage === 'manual_grading') {
      return 'manual';
    }
    if (
      workflowSubmission &&
      ['alignment_review', 'detect_review', 'parse_review'].includes(workflowSubmission.stage)
    ) {
      return 'needsReview';
    }
    if (
      workflowSubmission &&
      !['intake_ready', 'stopped'].includes(workflowSubmission.stage)
    ) {
      return 'processing';
    }
    return 'ready';
  }

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
  }

  function progressForSubmission(submission: StudentWorkflowSubmission): number {
    return workflowProgressByStudentRef.get(submission.studentRef)?.progress ??
      stageProgressValue(submission.stage);
  }

  function rosterDisplayNameFallback(): string {
    return lmsRosterEnabled &&
      (rosterCacheState.status === 'idle' || rosterCacheState.status === 'loading' || rosterBindingsLoading)
      ? 'Loading student'
      : 'Unknown student';
  }

  function scopedQuestionProgressForSubmission(
    submission: StudentWorkflowSubmission
  ): WorkflowScopedQuestionProgressEntry | null {
    const scopedProgress = workflowScopedQuestionProgressByStudentRef.get(submission.studentRef);
    if (!scopedProgress || scopedProgress.stage !== submission.stage) {
      return null;
    }
    return scopedProgress;
  }

  function questionOrdinal(questionId: string | null): number | null {
    if (!questionId) {
      return null;
    }
    const question = questionById.get(questionId);
    if (question && Number.isFinite(question.questionNumber) && question.questionNumber > 0) {
      return question.questionNumber;
    }
    const sortedQuestionIds = [...questionById.values()]
      .sort((a, b) => a.questionNumber - b.questionNumber)
      .map((item) => item.questionId);
    const index = sortedQuestionIds.indexOf(questionId);
    return index >= 0 ? index + 1 : null;
  }

  function progressTitleForSubmission(submission: StudentWorkflowSubmission): string {
    const baseLabel = stageLabel(submission.stage);
    const scopedProgress = scopedQuestionProgressForSubmission(submission);
    if (!scopedProgress) {
      return baseLabel;
    }
    const completed = Math.min(
      scopedProgress.completedQuestionIds.length,
      scopedProgress.totalQuestionCount
    );
    const parts = [
      baseLabel,
      `${completed}/${scopedProgress.totalQuestionCount} questions complete`
    ];
    if (scopedProgress.activeQuestionId) {
      const ordinal = questionOrdinal(scopedProgress.activeQuestionId);
      parts.push(
        ordinal !== null
          ? `active question Q${ordinal} (${scopedProgress.activeQuestionId})`
          : `active question ${scopedProgress.activeQuestionId}`
      );
    }
    return parts.join(' · ');
  }

  function workflowProgressMetrics(studentRef: string): { criterionCount: number; questionCount: number } | null {
    const submission = workflowByStudentRef.get(studentRef) ?? null;
    if (!submission) {
      return null;
    }
    const automationAnswers = submission.answers.filter((answer) => !(answer.manualGradingRequired ?? false));
    const questionIds = automationAnswers.map((answer) => answer.questionId);
    const relevantQuestions =
      questionIds.length > 0
        ? questionIds
            .map((questionId) => questionById.get(questionId) ?? null)
            .filter((question): question is NonNullable<typeof question> => question !== null)
        : workspaceState?.questions ?? [];
    const questionCount =
      automationAnswers.length > 0
        ? automationAnswers.length
        : Math.max(1, relevantQuestions.length);
    const criterionCount = relevantQuestions.reduce(
      (total, question) => total + (question.rubric?.criteria.length ?? 0),
      0
    );
    return {
      criterionCount: criterionCount > 0 ? criterionCount : questionCount,
      questionCount
    };
  }

  function syncWorkflowProgress(submissions: StudentWorkflowSubmission[]): void {
    const next = new Map<string, WorkflowProgressEntry>();
    const nextStageStudentRefs = new Map<string, string>();
    let changed = workflowProgressByStudentRef.size !== submissions.length;
    for (const submission of submissions) {
      nextStageStudentRefs.set(submission.stage, submission.studentRef);
      const existing = workflowProgressByStudentRef.get(submission.studentRef);
      if (submission.stage === 'failed') {
        const nextEntry = {
          stage: submission.stage,
          progress: existing?.progress ?? 0
        };
        next.set(submission.studentRef, nextEntry);
        changed = changed || existing?.stage !== nextEntry.stage || existing.progress !== nextEntry.progress;
        continue;
      }
      const range = stageProgressRange(submission.stage);
      const baseline = stageProgressValue(submission.stage);
      if (
        existing?.stage === submission.stage &&
        existing.progress >= range.start - stageProgressRangeEpsilon &&
        existing.progress <= range.end + stageProgressRangeEpsilon
      ) {
        next.set(submission.studentRef, existing);
        continue;
      }
      const nextEntry = {
        stage: submission.stage,
        progress: baseline
      };
      next.set(submission.studentRef, nextEntry);
      changed = changed || existing?.stage !== nextEntry.stage || existing.progress !== nextEntry.progress;
    }
    latestWorkflowStudentRefByStage = nextStageStudentRefs;
    if (changed) {
      workflowProgressByStudentRef = next;
      workflowProgressRevision += 1;
    }
    syncScopedQuestionProgress(submissions);
  }

  function syncScopedQuestionProgress(submissions: StudentWorkflowSubmission[]): void {
    if (workflowScopedQuestionProgressByStudentRef.size === 0) {
      return;
    }
    const activeStagesByStudentRef = new Map(
      submissions
        .filter((submission) => submission.stage === 'detect' || submission.stage === 'pii')
        .map((submission) => [submission.studentRef, submission.stage] as const)
    );
    const next = new Map(workflowScopedQuestionProgressByStudentRef);
    let changed = false;
    for (const [studentRef, scopedProgress] of next) {
      if (activeStagesByStudentRef.get(studentRef) !== scopedProgress.stage) {
        next.delete(studentRef);
        changed = true;
      }
    }
    if (changed) {
      workflowScopedQuestionProgressByStudentRef = next;
      workflowProgressRevision += 1;
    }
  }

  function readEventStudentRef(event: RuntimeJobEvent): string | null {
    const direct = event.payload.studentRef ?? event.payload.student_ref;
    if (typeof direct === 'string' && direct.trim().length > 0) {
      return direct.trim();
    }
    const scope = event.payload.scope;
    if (isRecord(scope)) {
      const scoped = scope.studentRef ?? scope.student_ref;
      if (typeof scoped === 'string' && scoped.trim().length > 0) {
        return scoped.trim();
      }
    }
    return null;
  }

  function readEventQuestionId(event: RuntimeJobEvent): string | null {
    const direct = event.payload.questionId ?? event.payload.question_id;
    if (typeof direct === 'string' && direct.trim().length > 0) {
      return direct.trim();
    }
    const scope = event.payload.scope;
    if (isRecord(scope)) {
      const scoped = scope.questionId ?? scope.question_id;
      if (typeof scoped === 'string' && scoped.trim().length > 0) {
        return scoped.trim();
      }
    }
    return null;
  }

  function defaultQuestionCountForStudent(studentRef: string): number {
    const submission = workflowByStudentRef.get(studentRef);
    if (submission && submission.answers.length > 0) {
      return submission.answers.length;
    }
    return Math.max(1, workspaceState?.questions?.length ?? 0);
  }

  function singleSubmissionInStage(stage: string): string | null {
    const matches = workflowSubmissions.filter((submission) => submission.stage === stage);
    return matches.length === 1 ? matches[0]!.studentRef : null;
  }

  function correlatedStudentRef(event: RuntimeJobEvent, stage: string): string | null {
    const fromPayload = readEventStudentRef(event);
    if (fromPayload) {
      if (event.jobId) {
        workflowStudentRefByJobId.set(event.jobId, fromPayload);
      }
      latestWorkflowStudentRefByStage.set(stage, fromPayload);
      return fromPayload;
    }
    if (event.jobId) {
      const fromJob = workflowStudentRefByJobId.get(event.jobId);
      if (fromJob) {
        return fromJob;
      }
    }
    const fromStage = latestWorkflowStudentRefByStage.get(stage);
    if (fromStage) {
      return fromStage;
    }
    return singleSubmissionInStage(stage);
  }

  function setWorkflowProgress(studentRef: string, stage: string, progress: number): void {
    workflowProgressByStudentRef = new Map(workflowProgressByStudentRef).set(studentRef, {
      stage,
      progress
    });
    workflowProgressRevision += 1;
    latestWorkflowStudentRefByStage.set(stage, studentRef);
  }

  function commandProgressStateKey(event: RuntimeJobEvent, studentRef: string, stage: string): string {
    return event.jobId ?? `${studentRef}:${stage}:${event.commandName}`;
  }

  function rememberWorkflowStudentRef(
    event: RuntimeJobEvent,
    studentRef: string,
    stage: string
  ): void {
    if (event.jobId) {
      workflowStudentRefByJobId.set(event.jobId, studentRef);
    }
    latestWorkflowStudentRefByStage.set(stage, studentRef);
  }

  function clearWorkflowCommandTracking(event: RuntimeJobEvent, progressStateKey: string): void {
    workflowCommandProgressByJobId.delete(progressStateKey);
    if (event.jobId) {
      workflowStudentRefByJobId.delete(event.jobId);
    }
  }

  function clearScopedQuestionProgress(studentRef: string, stage: string): void {
    const existing = workflowScopedQuestionProgressByStudentRef.get(studentRef);
    if (!existing || existing.stage !== stage) {
      return;
    }
    const next = new Map(workflowScopedQuestionProgressByStudentRef);
    next.delete(studentRef);
    workflowScopedQuestionProgressByStudentRef = next;
    workflowProgressRevision += 1;
  }

  function applyScopedQuestionProgress(
    event: RuntimeJobEvent,
    studentRef: string,
    stage: string
  ): void {
    if (stage !== 'detect' && stage !== 'pii') {
      return;
    }
    const innerEvent = event.payload.event;
    if (innerEvent !== 'item_started' && innerEvent !== 'item_completed') {
      return;
    }
    const questionId = readEventQuestionId(event);
    if (!questionId) {
      return;
    }
    const existing = workflowScopedQuestionProgressByStudentRef.get(studentRef);
    const completedQuestionIds = new Set(
      existing?.stage === stage ? existing.completedQuestionIds : []
    );
    if (innerEvent === 'item_completed') {
      completedQuestionIds.add(questionId);
    }
    const totalQuestionCount = Math.max(
      existing?.totalQuestionCount ?? defaultQuestionCountForStudent(studentRef),
      completedQuestionIds.size,
      1
    );
    const next = new Map(workflowScopedQuestionProgressByStudentRef);
    next.set(studentRef, {
      stage,
      activeQuestionId: innerEvent === 'item_started' ? questionId : null,
      completedQuestionIds: [...completedQuestionIds],
      totalQuestionCount
    });
    workflowScopedQuestionProgressByStudentRef = next;
    workflowProgressRevision += 1;
  }

  function scopedQuestionInnerPercent(studentRef: string, stage: string): number | null {
    const scopedProgress = workflowScopedQuestionProgressByStudentRef.get(studentRef);
    if (!scopedProgress || scopedProgress.stage !== stage) {
      return null;
    }
    const total = Math.max(1, scopedProgress.totalQuestionCount);
    return Math.trunc((Math.min(scopedProgress.completedQuestionIds.length, total) / total) * 100);
  }

  function handleWorkflowStateUpdated(event: RuntimeJobEvent): boolean {
    if (event.eventType !== 'workflow_state_updated') {
      return false;
    }
    const studentRef = readEventStudentRef(event);
    const stage = event.payload.stage;
    if (studentRef && typeof stage === 'string') {
      latestWorkflowStudentRefByStage.set(stage, studentRef);
      const existing = workflowProgressByStudentRef.get(studentRef);
      if (!existing || existing.stage !== stage) {
        setWorkflowProgress(studentRef, stage, stageProgressValue(stage));
      }
    }
    return true;
  }

  function handleWorkflowRuntimeEvent(event: RuntimeJobEvent): void {
    if (event.eventType === 'lms_roster_cache_updated') {
      void loadRosterCacheState();
      return;
    }
    if (handleWorkflowStateUpdated(event)) {
      return;
    }
    const commandStage = commandWorkflowStage(event.commandName);
    if (!commandStage) {
      return;
    }
    const studentRef = correlatedStudentRef(event, commandStage);
    if (!studentRef) {
      return;
    }
    rememberWorkflowStudentRef(event, studentRef, commandStage);
    const range = commandProgressRange(event.commandName, workflowProgressMetrics(studentRef));
    if (!range) {
      return;
    }
    const progressStateKey = commandProgressStateKey(event, studentRef, commandStage);
    if (event.eventType === 'job_started' || event.eventType === 'job_submitted') {
      workflowCommandProgressByJobId.set(progressStateKey, { totalStages: 1, currentStage: 1 });
      setWorkflowProgress(studentRef, commandStage, range.start);
      return;
    }
    if (event.eventType === 'job_progress') {
      applyScopedQuestionProgress(event, studentRef, commandStage);
      const prevState =
        workflowCommandProgressByJobId.get(progressStateKey) ?? null;
      const nextState = updateWorkflowCommandProgressState(prevState, event.payload);
      workflowCommandProgressByJobId.set(progressStateKey, nextState);
      const payload = event.payload;
      const innerPercent =
        scopedQuestionInnerPercent(studentRef, commandStage) ?? readCliProgressPercent(payload);
      if (innerPercent !== null) {
        setWorkflowProgress(
          studentRef,
          commandStage,
          workflowBarValueFromCommandProgress(range, nextState, innerPercent)
        );
      } else if (isSubstageBoundaryProgressPayload(payload, nextState)) {
        setWorkflowProgress(
          studentRef,
          commandStage,
          workflowBarValueFromCommandProgress(range, nextState, 0)
        );
      }
      return;
    }
    if (event.eventType === 'job_finished') {
      clearWorkflowCommandTracking(event, progressStateKey);
      clearScopedQuestionProgress(studentRef, commandStage);
      setWorkflowProgress(studentRef, commandStage, range.end);
      return;
    }
    if (event.eventType === 'job_failed' || event.eventType === 'job_cancelled') {
      clearWorkflowCommandTracking(event, progressStateKey);
      clearScopedQuestionProgress(studentRef, commandStage);
    }
  }

  function showHome() {
    selectedStudentRef = null;
    shellMode = 'home';
    activeIntakeFilename = null;
  }

  function showIntake() {
    selectedStudentRef = null;
    shellMode = 'intake';
  }

  async function browsePdfForIntake() {
    const selection = await openDialog({
      multiple: true,
      title: 'Choose Student PDFs',
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    });
    const paths = (Array.isArray(selection) ? selection : typeof selection === 'string' ? [selection] : [])
      .filter((path): path is string => typeof path === 'string' && path.trim().length > 0)
      .map((path) => path.trim());
    if (paths.length === 0) return;
    showIntake();
    intakeSeedPaths = paths;
    intakeSeedVersion += 1;
  }

  async function loadRosterCacheState() {
    try {
      rosterCacheState = await getLmsRosterCacheState();
    } catch (err) {
      rosterCacheState = {
        status: 'error',
        projectPath: workspaceState?.project?.projectPath ?? null,
        lmsProvider: null,
        courseId: resolvedLmsCourseId,
        rows: [],
        lastError: String(err),
        idleReason: null
      };
    }
  }

  async function ensureSharedRosterCache() {
    if (!resolvedLmsCourseId) {
      rosterCacheState = {
        status: 'idle',
        projectPath: workspaceState?.project?.projectPath ?? null,
        lmsProvider: null,
        courseId: null,
        rows: [],
        lastError: null,
        idleReason: 'No LMS course linked.'
      };
      return;
    }
    try {
      rosterCacheState = await ensureLmsRosterPreload();
    } catch (err) {
      rosterCacheState = {
        status: 'error',
        projectPath: workspaceState?.project?.projectPath ?? null,
        lmsProvider: null,
        courseId: resolvedLmsCourseId,
        rows: [],
        lastError: String(err),
        idleReason: null
      };
    }
  }

  async function buildRosterBindings(
    rawRows: LmsRosterRow[],
    courseId: string | null
  ): Promise<RosterRowWithBinding[]> {
    if (!courseId) {
      return rawRows.map((row) => ({ ...row, bindingTokenHex: null }));
    }
    return Promise.all(
      rawRows.map(async (row) => {
        try {
          const bindingTokenHex = await computeLmsBindingToken(courseId, row.userId);
          return { ...row, bindingTokenHex };
        } catch {
          return { ...row, bindingTokenHex: null };
        }
      })
    );
  }

  function rosterVerificationMessage(bindings: RosterRowWithBinding[]): string | null {
    if (studentRoster.length === 0) {
      return null;
    }
    const missingVerificationTokens = bindings.filter((row) => !normalizedRosterBindingToken(row))
      .length;
    const liveTokens = new Set(
      bindings
        .map((row) => normalizedRosterBindingToken(row))
        .filter((token): token is string => token !== null && token.length > 0)
    );
    const persistedTokens = new Set(studentRoster.map((row) => row.bindingTokenHex));
    if (missingVerificationTokens > 0) {
      return rosterVerificationUnavailableMessage(missingVerificationTokens);
    }
    const mismatch =
      liveTokens.size !== persistedTokens.size ||
      [...persistedTokens].some((tokenValue) => !liveTokens.has(tokenValue));
    return mismatch ? rosterMismatchMessage(studentRoster, liveTokens, persistedTokens) : null;
  }

  function rosterMismatchMessage(
    rows: StudentRosterRow[],
    liveTokens: Set<string>,
    persistedTokens: Set<string>
  ): string {
    const persistedOnlyRows = rows
      .filter((row) => !liveTokens.has(row.bindingTokenHex))
      .map((row) => `${row.studentRef} (${row.bindingTokenHex.slice(0, 12)})`);
    const liveOnlyTokens = [...liveTokens]
      .filter((token) => !persistedTokens.has(token))
      .map((token) => token.slice(0, 12));
    const matchedCount = rows.length - persistedOnlyRows.length;
    const parts = [
      `Roster verification mismatch: matched ${matchedCount}/${rows.length} persisted students.`
    ];
    if (persistedOnlyRows.length > 0) {
      parts.push(`Persisted only: ${summarizeValues(persistedOnlyRows)}.`);
    }
    if (liveOnlyTokens.length > 0) {
      parts.push(`Live LMS only: ${summarizeValues(liveOnlyTokens)}.`);
    }
    if (
      rows.length > 0 &&
      matchedCount === 0 &&
      liveTokens.size === persistedTokens.size &&
      liveTokens.size > 0
    ) {
      parts.push(
        'Zero tokens overlapped. This usually means the LMS binding HMAC secret or linked course id changed, not that the roster membership changed.'
      );
    }
    return parts.join(' ');
  }

  function summarizeValues(values: string[], limit = 3): string {
    if (values.length === 0) return 'none';
    const preview = values.slice(0, limit).join(', ');
    return values.length > limit ? `${preview}, +${values.length - limit} more` : preview;
  }

  function rosterVerificationUnavailableMessage(missingCount: number): string {
    return `Live LMS roster could not be fully verified against the persisted project roster right now because token recomputation failed for ${missingCount} live roster row${missingCount === 1 ? '' : 's'}.`;
  }

  function intakeCompleteCount(items: StudentIntakeSummary[]): number {
    return items.filter((item) => (item.examPagePaths?.length ?? 0) > 0 && item.ingestStatus !== 'failed').length;
  }

  async function handleSidebarSelect(studentRef: string) {
    selectStudent(studentRef);
  }

  async function handleDeleteSubmission(
    studentRef: string,
    nextSelectedStudentRef: string | null
  ) {
    await onDeleteSubmission?.(studentRef, nextSelectedStudentRef);
    pendingDeleteStudent = null;
    selectedStudentRef = nextSelectedStudentRef;
    shellMode = 'home';
  }

  function nextStudentAfter(studentRef: string): string | null {
    const currentIndex = visibleRosterEntries.findIndex(
      (entry) => entry.student.studentRef === studentRef
    );
    if (currentIndex < 0) {
      return visibleRosterEntries.find((entry) => entry.student.studentRef !== studentRef)?.student
        .studentRef ?? null;
    }
    return (
      visibleRosterEntries[currentIndex + 1]?.student.studentRef ??
      visibleRosterEntries[currentIndex - 1]?.student.studentRef ??
      null
    );
  }

  function nextReviewStudentAfter(studentRef: string, stage: string): string | null {
    const currentIndex = visibleRosterEntries.findIndex(
      (entry) => entry.student.studentRef === studentRef
    );
    const hasPendingReview = (index: number) => {
      const entry = visibleRosterEntries[index];
      return (
        entry?.student.studentRef !== studentRef &&
        entry?.workflowSubmission?.stage === stage
      );
    };
    for (let index = Math.max(currentIndex, -1) + 1; index < visibleRosterEntries.length; index += 1) {
      if (hasPendingReview(index)) {
        return visibleRosterEntries[index].student.studentRef;
      }
    }
    for (let index = 0; index < Math.max(currentIndex, 0); index += 1) {
      if (hasPendingReview(index)) {
        return visibleRosterEntries[index].student.studentRef;
      }
    }
    return null;
  }

  function selectNextReviewStudentOrHome(studentRef: string, stage: string) {
    const nextStudentRef = nextReviewStudentAfter(studentRef, stage);
    if (nextStudentRef) {
      selectStudent(nextStudentRef);
      return;
    }
    showHome();
  }

  function requestSelectedDelete() {
    if (!selectedStudentRef || !onDeleteSubmission || busyAction !== null) {
      return;
    }
    pendingDeleteStudent = {
      studentRef: selectedStudentRef,
      displayName: selectedDisplayName || 'this student'
    };
  }

  async function confirmSelectedDelete() {
    if (!pendingDeleteStudent || !onDeleteSubmission || busyAction !== null) {
      return;
    }
    await handleDeleteSubmission(
      pendingDeleteStudent.studentRef,
      nextStudentAfter(pendingDeleteStudent.studentRef)
    );
  }

  async function handleSidebarBrowsePdf() {
    await browsePdfForIntake();
  }

  async function handleConfirmAlignment(studentRef: string, pages: StudentWorkflowAlignmentPage[]) {
    await onConfirmAlignment?.(studentRef, pages);
    selectNextReviewStudentOrHome(studentRef, 'alignment_review');
  }

  async function handleConfirmDetectReview(
    studentRef: string,
    resolutions: StudentWorkflowDetectReviewResolutionInput[]
  ) {
    await onConfirmDetectReview?.(studentRef, resolutions);
    selectNextReviewStudentOrHome(studentRef, 'detect_review');
  }

  async function handleConfirmParseReview(studentRef: string, questionId: string, correctedText: string) {
    await onConfirmParseReview?.(studentRef, questionId, correctedText);
  }

  onMount(() => {
    lastRosterPreloadKey = rosterPreloadKey;
    if (lmsRosterEnabled) {
      void ensureSharedRosterCache();
    }
    void loadRosterCacheState();
    return onRuntimeJobEvent(handleWorkflowRuntimeEvent);
  });

  $: if (rosterPreloadKey !== lastRosterPreloadKey) {
    lastRosterPreloadKey = rosterPreloadKey;
    if (lmsRosterEnabled) {
      void ensureSharedRosterCache();
    }
  }

  $: if (rosterCacheState.status !== 'ready') {
    rosterRows = [];
    lastRosterBindingKey = '';
    rosterBindingKeyInFlight = null;
    rosterVerificationError = null;
  }

  $: if (rosterCacheState.status === 'ready') {
    const bindingKey = JSON.stringify({
      courseId: rosterCacheState.courseId,
      rows: rosterCacheState.rows
    });
    if (bindingKey !== lastRosterBindingKey) {
      lastRosterBindingKey = bindingKey;
      rosterBindingKeyInFlight = rosterCacheState.rows.length > 0 ? bindingKey : null;
      void buildRosterBindings(rosterCacheState.rows, rosterCacheState.courseId).then((bindings) => {
        if (bindingKey !== lastRosterBindingKey) {
          return;
        }
        // eslint-disable-next-line svelte/infinite-reactive-loop -- guarded by bindingKey; applies only the newest async roster snapshot.
        rosterRows = bindings;
        // eslint-disable-next-line svelte/infinite-reactive-loop -- guarded by bindingKey; clears loading for the same roster snapshot.
        rosterBindingKeyInFlight = null;
      });
    }
  }

  $: if (rosterCacheState.status === 'ready' && !rosterBindingsLoading) {
    rosterVerificationError = rosterVerificationMessage(rosterRows);
  }

  $: syncWorkflowProgress(workflowSubmissions);
</script>

<div class="flex h-full min-h-0">
  <StudentWorkflowSidebar
    entries={sidebarEntries}
    {selectedStudentRef}
    {busyAction}
    {rosterBusy}
    rosterActionLabel={rosterActionLabel}
    rosterActionDisabled={rosterActionDisabled}
    {rosterError}
    onselect={handleSidebarSelect}
    onbrowsepdf={() => void handleSidebarBrowsePdf()}
    onrefreshroster={() => void ensureSharedRosterCache()}
    onintakedrop={(paths) => {
      showIntake();
      intakeSeedPaths = paths;
      intakeSeedVersion += 1;
    }}
  />

  <section
    class={`flex min-h-0 flex-1 flex-col bg-surface-panel px-6 pb-6 ${compactReviewHeader ? 'pt-2' : 'pt-6'} ${currentView === 'intake' ? 'overflow-y-auto' : ''}`}
  >
    {#if currentView === 'intake'}
      <div class="mb-4 flex items-center gap-4">
        <div class="w-40">
          <DesktopButton class="whitespace-nowrap" variant="ghost" size="compact" onclick={showHome}>
            Back to workflow
          </DesktopButton>
        </div>
        <div
          class="min-w-0 flex-1 truncate text-center text-base font-semibold text-workspace-text-primary"
          title={activeIntakeFilename ? `Student Intake - ${activeIntakeFilename}` : 'Student Intake Processor'}
        >
          {activeIntakeFilename ? `Student Intake - ${activeIntakeFilename}` : 'Student Intake Processor'}
        </div>
        <div class="w-40"></div>
      </div>

      <StudentIntakeWorkspace
        {busyAction}
        {prerequisitesMet}
        lmsCourseId={resolvedLmsCourseId}
        sharedRosterStatus={rosterCacheState.status}
        sharedRosterRows={rosterCacheState.rows}
        sharedRosterMessage={rosterCacheState.lastError ?? rosterCacheState.idleReason ?? null}
        onEnsureRosterCache={ensureSharedRosterCache}
        existingIntakeItems={intakeItems}
        expectedPageCount={expectedTemplatePageCount}
        {onFinalizeSubmission}
        onActiveFileChange={(filename) => {
          activeIntakeFilename = filename;
        }}
        onSubmissionCompleted={async ({ studentRef }) => {
          selectedStudentRef = studentRef;
          await loadRosterCacheState();
        }}
        seedPaths={intakeSeedPaths}
        seedVersion={intakeSeedVersion}
      />
    {:else if currentView === 'alignmentReview'}
      <AlignmentReviewView
        intakeItem={selectedIntakeItem}
        submission={selectedWorkflowSubmission!}
        templatePreviewArtifacts={workspaceState.templatePreviewArtifacts}
        displayName={selectedDisplayName}
        {busyAction}
        saving={reviewSaveBusy[saveBusyKey('alignment', selectedWorkflowSubmission!.studentRef)] === true}
        onconfirm={(pages) => handleConfirmAlignment(selectedWorkflowSubmission!.studentRef, pages)}
        ondelete={onDeleteSubmission && selectedHasSubmission ? requestSelectedDelete : null}
        deleteDisabled={busyAction !== null}
        onback={showHome}
      />
    {:else if currentView === 'submissionPages'}
      <SubmissionPagesView
        intakeItem={selectedIntakeItem}
        submission={selectedWorkflowSubmission}
        displayName={selectedDisplayName}
        stageProgress={selectedStageProgress}
        expectedPageCount={expectedTemplatePageCount}
        onSavePageOrder={onSaveStudentIntakePageOrder}
        ondelete={onDeleteSubmission && selectedHasSubmission ? requestSelectedDelete : null}
        deleteDisabled={busyAction !== null}
        onback={showHome}
      />
    {:else if currentView === 'detectReview'}
      <DetectReviewView
        intakeItem={selectedIntakeItem}
        submission={selectedWorkflowSubmission!}
        displayName={selectedDisplayName}
        {busyAction}
        saving={reviewSaveBusy[saveBusyKey('detect', selectedWorkflowSubmission!.studentRef)] === true}
        onconfirm={onConfirmDetectReview
          ? (resolutions) =>
              handleConfirmDetectReview(selectedWorkflowSubmission!.studentRef, resolutions)
          : null}
        ondelete={onDeleteSubmission && selectedHasSubmission ? requestSelectedDelete : null}
        deleteDisabled={busyAction !== null}
        onback={showHome}
      />
    {:else if currentView === 'questionDetail'}
      <QuestionDetailView
        submission={selectedWorkflowSubmission}
        displayName={selectedDisplayName}
        {busyAction}
        {reviewSaveBusy}
        stageProgress={selectedStageProgress}
        onconfirmparsereview={onConfirmParseReview ? handleConfirmParseReview : null}
        onsavecriterionscore={onSaveCriterionScore ?? undefined}
        ondelete={onDeleteSubmission && selectedHasSubmission ? requestSelectedDelete : null}
        deleteDisabled={busyAction !== null}
        onback={showHome}
      />
    {:else}
      <StudentWorkflowBoard
        courseCode={workspaceState?.project?.courseCode ?? 'Course'}
        displayName={workspaceState?.project?.displayName ?? 'Exam'}
        intakeComplete={intakeCompleteCount(intakeItems)}
        {processingCount}
        attentionCount={attentionItems.length}
        {gradedCount}
        {readyCount}
        canonicalReadyCount={canonicalReadyRows.length}
        busyActionLabel={studentWorkflowRunning ? 'Running…' : null}
        recoveryAvailable={studentWorkflowRecoveryAvailable}
        recoveryBusy={busyAction === 'studentWorkflowRecovery'}
        {stopWorkflowBusy}
        {attentionItems}
        {canonicalReadyRows}
        onSelectStudent={selectStudent}
        onBeginWorkflow={onBeginWorkflow}
        onStopWorkflow={onStopWorkflow}
        onRecoverWorkflow={onRecoverWorkflow}
      />
    {/if}
  </section>

  {#if pendingDeleteStudent}
    <ConfirmDialog
      open
      destructive
      title="Delete student submission?"
      description={`This removes the current submission and workflow state for ${pendingDeleteStudent.displayName}. LMS roster identity is preserved.`}
      confirmLabel="Delete submission"
      busy={busyAction !== null}
      onCancel={() => {
        pendingDeleteStudent = null;
      }}
      onConfirm={confirmSelectedDelete}
    />
  {/if}
</div>
