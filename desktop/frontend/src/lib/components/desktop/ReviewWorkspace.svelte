<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { writable } from 'svelte/store';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import {
    AlertCircleIcon,
    AiMagicIcon,
    Cancel01Icon,
    Delete02Icon,
    InformationCircleIcon,
    ApproximatelyEqualIcon,
    Image01Icon,
    Loading03Icon,
    PercentCircleIcon,
    Queue01Icon,
    Tick01Icon
  } from '@hugeicons/core-free-icons';
  import { scheduleAutomaticRubricEnsureAfterUiPaint } from '$lib/automaticRubricEnsure';
  import { toDesktopAssetUrl } from '$lib/desktop';
  import { onRuntimeJobEvent } from '$lib/stores/shell';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import {
    questionAnalysisRunning,
    questionAnalysisSidebarState,
    reviewSidebarIconState,
    type AnalysisJobState,
    type RubricGenerationJobState
  } from '$lib/reviewSidebarState';
  import {
    questionAnalysisIsComplete,
    questionAnalysisStatusKind,
    type ExamWorkspaceState,
    type QuestionEdit,
    type RubricCriterion,
    type RuntimeJobEvent,
    type WorkspaceWarning
  } from '$lib/types';
  import {
    ConfirmDialog,
    DesktopButton,
    DesktopPopover,
    IconButton,
    InlineMessage,
    PagePreviewFrame,
    TextareaField,
    TextField,
    ToneIcon
  } from './ui';

  type ApprovedEditImpact = 'minor' | 'grading';

  export let workspaceState: ExamWorkspaceState;
  export let questionDrafts: QuestionEdit[] = [];
  export let selectedQuestionId: string | null = null;
  export let busyAction: BusyAction = null;
  export let hasUnsavedQuestionEdits = false;
  export let onSelectQuestion: ((questionId: string) => void) | null = null;
  export let onUpdateQuestion:
    | ((questionId: string, patch: Partial<Omit<QuestionEdit, 'questionId'>>) => void)
    | null = null;
  export let onSaveReviewChanges:
    | ((
        questionId: string,
        criteria: RubricCriterion[],
        saveQuestions: boolean,
        saveRubric: boolean,
        approvedEditImpact?: ApprovedEditImpact
      ) => void | Promise<void>)
    | null = null;
  export let onDiscardReviewChanges: ((questionId: string) => void) | null = null;
  export let onGenerateRubric: ((questionId: string) => void | Promise<void>) | null = null;
  export let onSaveRubric:
    | ((questionId: string, criteria: RubricCriterion[], approve: boolean) => void | Promise<void>)
    | null = null;
  export let onReAnalyze: ((questionId: string) => void | Promise<void>) | null = null;
  export let analysisInProgress = false;
  export let analysisJobByQuestion: Record<string, AnalysisJobState> = {};

  let imagePopoverOpen = false;
  let pointsMismatchFlash = false;
  let approvedSaveDecisionOpen = false;
  let pendingDeleteCriterionIndex: number | null = null;
  let questionContextHelpOpen = false;
  let rubricHelpOpen = false;

  const editableCriteria = writable<RubricCriterion[]>([]);
  let selectedCriterionQuestionId: string | null = null;
  let selectedCriterionKey: string | null = null;

  /** `exam.generate-rubric` UI: queued vs running per question (host dedupes enqueue). */
  let rubricGenByQuestion: Record<string, RubricGenerationJobState> = {};
  const rubricGenJobToQuestion = new Map<string, string>();

  function rubricGenPayloadQuestionId(payload: Record<string, unknown>): string | null {
    const raw = payload.questionId ?? payload.question_id;
    return typeof raw === 'string' && raw.length > 0 ? raw : null;
  }

  function applyRubricGenRuntimeEvent(event: RuntimeJobEvent) {
    if (event.commandName !== 'exam.generate-rubric' || !event.jobId) {
      return;
    }
    const fromPayload = rubricGenPayloadQuestionId(event.payload);
    const qid = fromPayload ?? rubricGenJobToQuestion.get(event.jobId) ?? null;
    if (!qid) {
      return;
    }
    rubricGenJobToQuestion.set(event.jobId, qid);

    if (event.eventType === 'job_queued') {
      rubricGenByQuestion = { ...rubricGenByQuestion, [qid]: 'queued' };
      return;
    }
    if (event.eventType === 'job_submitted' || event.eventType === 'job_started') {
      rubricGenByQuestion = { ...rubricGenByQuestion, [qid]: 'running' };
      return;
    }
    if (
      event.eventType === 'job_finished' ||
      event.eventType === 'job_failed' ||
      event.eventType === 'job_cancelled'
    ) {
      rubricGenJobToQuestion.delete(event.jobId);
      if (rubricGenByQuestion[qid]) {
        const next = { ...rubricGenByQuestion };
        delete next[qid];
        rubricGenByQuestion = next;
      }
    }
  }

  function handleReviewRuntimeEvent(event: RuntimeJobEvent) {
    applyRubricGenRuntimeEvent(event);
  }

  let stopRubricGenEvents: (() => void) | null = null;
  onMount(() => {
    stopRubricGenEvents = onRuntimeJobEvent(handleReviewRuntimeEvent);
    scheduleAutomaticRubricEnsureAfterUiPaint();
  });
  onDestroy(() => {
    stopRubricGenEvents?.();
  });

  function patchQuestionContext(questionId: string, next: string) {
    onUpdateQuestion?.(questionId, { questionContext: next });
  }

  $: orderedQuestionDrafts = [...questionDrafts].sort(
    (left, right) =>
      left.questionNumber - right.questionNumber ||
      left.pageNumber - right.pageNumber ||
      left.questionId.localeCompare(right.questionId)
  );
  $: selectedQuestionDraft =
    orderedQuestionDrafts.find((question) => question.questionId === selectedQuestionId) ??
    orderedQuestionDrafts[0] ??
    null;
  $: selectedQuestionRecord =
    workspaceState.questions.find((question) => question.questionId === selectedQuestionDraft?.questionId) ??
    null;
  $: selectedAnalysis = selectedQuestionRecord?.analysis ?? {
    status: 'not_started',
    questionTextClean: null,
    questionContext: null,
    warnings: [],
    latestJobId: null
  };
  $: selectedRubric = selectedQuestionRecord?.rubric ?? {
    status: 'not_started',
    criteria: [],
    warnings: [],
    approvedAt: null,
    latestJobId: null
  };
  $: selectedRubricJobState =
    selectedQuestionDraft?.questionId ? rubricGenByQuestion[selectedQuestionDraft.questionId] : null;
  $: selectedRubricGenerationActive = selectedRubricJobState === 'queued' || selectedRubricJobState === 'running';
  $: selectedRubricStatusLabel = selectedRubricJobState ?? selectedRubric.status;
  $: selectedAnalysisComplete = questionAnalysisIsComplete(selectedAnalysis);
  $: selectedAnalysisJobState =
    selectedQuestionDraft?.questionId ? analysisJobByQuestion[selectedQuestionDraft.questionId] : null;
  $: selectedAnalysisInProgress = questionAnalysisRunning(
    selectedQuestionRecord ?? undefined,
    analysisInProgress,
    selectedAnalysisJobState
  );
  $: selectedAnalysisFailed = questionAnalysisStatusKind(selectedAnalysis) === 'failed';
  /** Bumps when rubric data is regenerated so the editable store resyncs even if Svelte misses nested updates. */
  $: rubricEditorSyncKey = [
    selectedQuestionDraft?.questionId ?? '',
    selectedRubric.latestJobId ?? '',
    selectedRubric.status,
    selectedRubric.criteria?.length ?? 0
  ].join('|');
  $: {
    void rubricEditorSyncKey;
    editableCriteria.set(selectedRubric.criteria.map((criterion) => ({ ...criterion })));
  }
  $: hasUnsavedRubricEdits =
    JSON.stringify($editableCriteria) !== JSON.stringify(selectedRubric.criteria ?? []);
  $: hasUnsavedReviewChanges = hasUnsavedQuestionEdits || hasUnsavedRubricEdits;
  $: selectedRubricApproved =
    selectedRubric.status === 'approved' || selectedRubric.approvedAt != null;
  $: selectedQuestionBasisChanged =
    selectedQuestionDraft != null &&
    selectedQuestionRecord != null &&
    (selectedQuestionDraft.text !== selectedQuestionRecord.text ||
      selectedQuestionDraft.maxPoints !== selectedQuestionRecord.maxPoints ||
      selectedQuestionDraft.questionContext !== (selectedQuestionRecord.analysis?.questionContext ?? ''));
  $: selectedRubricStructuralChange =
    selectedRubric.criteria.length !== $editableCriteria.length ||
    selectedRubric.criteria.some(
      (criterion, index) =>
        criterion.criterionId !== $editableCriteria[index]?.criterionId ||
        criterion.points !== $editableCriteria[index]?.points
    );
  $: minorApprovedSaveAllowed =
    selectedRubricApproved &&
    hasUnsavedReviewChanges &&
    !selectedRubricStructuralChange &&
    selectedQuestionDraft?.maxPoints === selectedQuestionRecord?.maxPoints;
  $: criterionPointsSum = $editableCriteria.reduce((sum, c) => sum + (c.points ?? 0), 0);
  $: maxPointsForSelectedQuestion = selectedQuestionDraft?.maxPoints ?? null;
  $: pointsMismatchMessage = (() => {
    if (!maxPointsForSelectedQuestion) {
      return $editableCriteria.length > 0 ? 'Set question max points before approving.' : null;
    }
    if (maxPointsForSelectedQuestion <= 0) {
      return 'Question max points must be greater than zero.';
    }
    if (criterionPointsSum !== maxPointsForSelectedQuestion) {
      return `Criteria points sum to ${criterionPointsSum}, but question max points is ${maxPointsForSelectedQuestion}.`;
    }
    return null;
  })();
  $: syncSelectedCriterionSelection(
    $editableCriteria,
    selectedQuestionDraft?.questionId ?? null
  );
  $: selectedCriterionIndex = $editableCriteria.findIndex(
    (criterion, index) => criterionSelectionKey(criterion, index) === selectedCriterionKey
  );
  $: selectedCriterion =
    selectedCriterionIndex >= 0 ? $editableCriteria[selectedCriterionIndex] : null;
  $: selectedCriterionWarnings =
    selectedCriterionIndex >= 0 ? warningsForCriterionIndex(selectedCriterionIndex) : [];
  $: selectedCriterionPointOptions = selectedCriterion
    ? pointOptionsForCriterion(selectedCriterion)
    : [];
  $: pendingDeleteCriterion =
    pendingDeleteCriterionIndex != null
      ? $editableCriteria[pendingDeleteCriterionIndex] ?? null
      : null;

  function criterionSelectionKey(criterion: RubricCriterion, index: number): string {
    return criterion.criterionId ? `id:${criterion.criterionId}` : `index:${index}`;
  }

  function criterionDisplayLabel(criterion: RubricCriterion, index: number): string {
    const label = criterion.label.trim();
    return label.length > 0 ? label : `Criterion ${index + 1}`;
  }

  function criterionPointsLabel(points: number | null | undefined): string {
    const value = points ?? 0;
    return `${value} pt${value === 1 ? '' : 's'}`;
  }

  function criterionWarningTitle(warnings: WorkspaceWarning[]): string {
    return warnings.map((warning) => warning.message).join('\n');
  }

  function pointOptionsForCriterion(criterion: RubricCriterion): number[] {
    const currentPoints = Math.max(0, criterion.points ?? 0);
    const maxPoints = Math.max(currentPoints, maxPointsForSelectedQuestion ?? 10, 1);
    return Array.from({ length: maxPoints }, (_, index) => index + 1);
  }

  function syncSelectedCriterionSelection(criteria: RubricCriterion[], questionId: string | null) {
    if (!questionId) {
      selectedCriterionQuestionId = null;
      selectedCriterionKey = null;
      return;
    }
    if (criteria.length === 0) {
      selectedCriterionQuestionId = questionId;
      selectedCriterionKey = null;
      return;
    }
    const keys = criteria.map((criterion, index) => criterionSelectionKey(criterion, index));
    if (selectedCriterionQuestionId !== questionId || !selectedCriterionKey) {
      selectedCriterionQuestionId = questionId;
      selectedCriterionKey = keys[0] ?? null;
      return;
    }
    if (!keys.includes(selectedCriterionKey)) {
      selectedCriterionKey = keys[0] ?? null;
    }
  }

  function selectCriterion(index: number) {
    const criterion = $editableCriteria[index];
    if (!criterion) {
      selectedCriterionKey = null;
      return;
    }
    selectedCriterionQuestionId = selectedQuestionDraft?.questionId ?? null;
    selectedCriterionKey = criterionSelectionKey(criterion, index);
  }

  function updateCriterion(index: number, patch: Partial<RubricCriterion>) {
    editableCriteria.update((criteria) =>
      criteria.map((criterion, currentIndex) =>
        currentIndex === index ? { ...criterion, ...patch } : criterion
      )
    );
  }

  function addCriterion() {
    editableCriteria.update((criteria) => {
      const newCriterion: RubricCriterion = {
        criterionId: '',
        label: '',
        points: 1,
        partialCreditGuidance: '',
        source: 'manual'
      };
      selectedCriterionQuestionId = selectedQuestionDraft?.questionId ?? null;
      selectedCriterionKey = criterionSelectionKey(newCriterion, criteria.length);
      return [...criteria, newCriterion];
    });
  }

  function removeCriterion(index: number) {
    editableCriteria.update((criteria) => {
      const next = criteria.filter((_, i) => i !== index);
      selectedCriterionQuestionId = selectedQuestionDraft?.questionId ?? null;
      if (next.length === 0) {
        selectedCriterionKey = null;
        return next;
      }
      const nextIndex = Math.min(index, next.length - 1);
      selectedCriterionKey = criterionSelectionKey(next[nextIndex]!, nextIndex);
      return next;
    });
  }

  function requestRemoveCriterion(index: number) {
    if (!$editableCriteria[index]) {
      return;
    }
    pendingDeleteCriterionIndex = index;
  }

  function cancelRemoveCriterion() {
    pendingDeleteCriterionIndex = null;
  }

  function confirmRemoveCriterion() {
    if (pendingDeleteCriterionIndex == null) {
      return;
    }
    removeCriterion(pendingDeleteCriterionIndex);
    pendingDeleteCriterionIndex = null;
  }

  function rubricApprovedForQuestion(questionId: string): boolean {
    const rubric = workspaceState.questions.find((question) => question.questionId === questionId)?.rubric;
    return rubric?.status === 'approved' || rubric?.approvedAt != null;
  }

  function warningsForCriterionIndex(index: number): WorkspaceWarning[] {
    return selectedRubric.warnings.filter((warning) => {
      if (!warning.scope) return false;
      try {
        const scope = JSON.parse(warning.scope);
        const criteria: unknown = scope.criteria;
        return Array.isArray(criteria) && criteria.includes(index + 1);
      } catch {
        return false;
      }
    });
  }

  function nextUnapprovedQuestionId(currentQuestionId: string): string | null {
    const currentIndex = orderedQuestionDrafts.findIndex(
      (question) => question.questionId === currentQuestionId
    );
    if (currentIndex === -1 || orderedQuestionDrafts.length < 2) {
      return null;
    }
    const searchOrder = [
      ...orderedQuestionDrafts.slice(currentIndex + 1),
      ...orderedQuestionDrafts.slice(0, currentIndex)
    ];
    const next = searchOrder.find((question) => !rubricApprovedForQuestion(question.questionId));
    return next?.questionId ?? null;
  }

  async function approveRubricAndAdvance() {
    if (!selectedQuestionDraft || !onSaveRubric) {
      return;
    }
    if (pointsMismatchMessage) {
      pointsMismatchFlash = true;
      setTimeout(() => { pointsMismatchFlash = false; }, 2000);
      return;
    }
    await onSaveRubric(selectedQuestionDraft.questionId, $editableCriteria, true);
    const nextQuestionId = nextUnapprovedQuestionId(selectedQuestionDraft.questionId);
    if (nextQuestionId) {
      onSelectQuestion?.(nextQuestionId);
    }
  }

  function requestSaveReviewChanges() {
    if (!selectedQuestionDraft || !onSaveReviewChanges) {
      return;
    }
    if (selectedRubricApproved && hasUnsavedReviewChanges) {
      approvedSaveDecisionOpen = true;
      return;
    }
    void onSaveReviewChanges(
      selectedQuestionDraft.questionId,
      $editableCriteria,
      hasUnsavedQuestionEdits,
      hasUnsavedRubricEdits
    );
  }

  function discardApprovedReviewChanges() {
    if (!selectedQuestionDraft) {
      return;
    }
    editableCriteria.set(selectedRubric.criteria.map((criterion) => ({ ...criterion })));
    onDiscardReviewChanges?.(selectedQuestionDraft.questionId);
    approvedSaveDecisionOpen = false;
  }

  function saveApprovedReviewChanges(impact: ApprovedEditImpact) {
    if (!selectedQuestionDraft || !onSaveReviewChanges) {
      return;
    }
    approvedSaveDecisionOpen = false;
    void onSaveReviewChanges(
      selectedQuestionDraft.questionId,
      $editableCriteria,
      hasUnsavedQuestionEdits,
      hasUnsavedRubricEdits,
      impact
    );
  }

  function rescindApprovedRubric() {
    if (!selectedQuestionDraft || !onSaveReviewChanges || hasUnsavedReviewChanges) {
      return;
    }
    void onSaveReviewChanges(
      selectedQuestionDraft.questionId,
      $editableCriteria,
      false,
      true,
      'grading'
    );
  }

</script>

<div class="grid h-full min-h-0 grid-cols-[20rem_minmax(0,1fr)]">
  <aside class="flex min-h-0 flex-col border-r border-workspace-sidebar-border bg-surface-sidebar">
    <div class="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-4">
      {#if orderedQuestionDrafts.length > 0}
        {#each orderedQuestionDrafts as question (question.questionId)}
          {@const questionRecord = workspaceState.questions.find(
            (record) => record.questionId === question.questionId
          )}
          {@const rubricGen = rubricGenByQuestion[question.questionId]}
          {@const analysisJobState = analysisJobByQuestion[question.questionId] ?? null}
          {@const criteriaOverride =
            selectedQuestionDraft?.questionId === question.questionId ? $editableCriteria : null}
          {@const iconState = reviewSidebarIconState({
            question: questionRecord,
            analysisJobState,
            rubricJobState: rubricGen,
            criteriaOverride,
            analysisInProgress
          })}
          {@const analysisIconState = questionAnalysisSidebarState(
            questionRecord,
            analysisInProgress,
            analysisJobState
          )}
          <button
            class={`relative w-full rounded-2xl border px-4 py-4 text-left transition-colors ${
              selectedQuestionDraft?.questionId === question.questionId
                ? 'border-workspace-border-strong bg-workspace-sidebar-active'
                : 'border-workspace-sidebar-border hover:bg-workspace-sidebar-hover'
            }`}
            type="button"
            onclick={() => {
              onSelectQuestion?.(question.questionId);
            }}
          >
            <div class="flex items-start justify-between gap-3">
              <div class="flex min-w-0 items-start gap-2">
                {#key `${question.questionId}:${iconState}`}
                  {#if iconState === 'analysis-running' || iconState === 'rubric-running'}
                    <span
                      class="inline-flex h-6 w-6 shrink-0 items-center justify-center text-workspace-text-muted"
                      aria-label={iconState === 'analysis-running' ? 'Analyzing question' : 'Generating rubric'}
                      title={iconState === 'analysis-running' ? 'Analyzing question' : 'Generating rubric'}
                    >
                      <HugeiconsIcon icon={Loading03Icon} class="h-4 w-4 animate-spin" aria-hidden="true" />
                    </span>
                  {:else if iconState === 'analysis-queued' || iconState === 'rubric-queued'}
                    <span
                      class="inline-flex h-6 w-6 shrink-0 items-center justify-center text-workspace-text-muted"
                      aria-label={analysisIconState === 'analysis-queued' ? 'Question analysis queued' : 'Rubric generation queued'}
                      title={analysisIconState === 'analysis-queued' ? 'Question analysis queued' : 'Rubric generation queued'}
                    >
                      <HugeiconsIcon icon={Queue01Icon} class="h-4 w-4" aria-hidden="true" />
                    </span>
                  {:else if iconState === 'rubric-error'}
                    <ToneIcon tone="error" icon={Cancel01Icon} label="Rubric error" />
                  {:else if iconState === 'rubric-warning'}
                    <ToneIcon tone="warning" icon={ApproximatelyEqualIcon} label="Rubric warning" />
                  {:else if iconState === 'rubric-approved'}
                    <ToneIcon tone="success" icon={Tick01Icon} label="Rubric approved" />
                  {/if}
                {/key}
                <span class="shell-body font-semibold text-workspace-text-primary">Question {question.questionNumber}</span>
              </div>
              <span class="shell-meta shrink-0 text-workspace-text-muted">p{question.pageNumber}</span>
            </div>
            <div class="mt-2 line-clamp-2 shell-body leading-6 text-workspace-text-secondary">
              {question.text}
            </div>
          </button>
        {/each}
      {:else}
        <div class="px-2 py-4 shell-body text-workspace-text-muted">No questions are available yet.</div>
      {/if}
    </div>
  </aside>

  <section class="relative flex min-h-0 flex-col bg-surface-panel">
    {#if selectedQuestionDraft && selectedQuestionRecord}
      <div class="flex-1 overflow-y-auto px-8 py-8">
        <div class="flex items-start justify-between gap-6">
          <div class="flex min-w-0 flex-wrap items-center gap-x-6 gap-y-2">
            <div class="shell-title-lg font-medium tracking-tight text-workspace-text-primary">
              Question {selectedQuestionDraft.questionNumber}
            </div>
            <div class="flex flex-col gap-0.5 text-xs leading-4 text-workspace-text-secondary">
              <div>Analysis status: <span class="font-medium text-workspace-text-primary">{selectedAnalysis.status}</span></div>
              <div>Rubric status: <span class="font-medium text-workspace-text-primary">{selectedRubricStatusLabel}</span></div>
            </div>
          </div>
          <div class="flex items-center gap-3">
            <div class="flex items-center gap-4">
              <div class="shell-body-lg text-workspace-text-secondary">Points</div>
              <TextField
                class="w-20"
                controlClass="h-14 rounded-xl bg-surface-card-control px-4 text-center text-3xl font-semibold [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                type="number"
                min="0"
                step="1"
                value={selectedQuestionDraft.maxPoints ?? ''}
                disabled={!selectedAnalysisComplete}
                oninput={(event: Event) => {
                  const raw = (event.currentTarget as HTMLInputElement).value.trim();
                  const parsed = raw === '' ? null : Number.parseInt(raw, 10);
                  onUpdateQuestion?.(selectedQuestionDraft.questionId, {
                    maxPoints: parsed === null || Number.isNaN(parsed) ? null : parsed
                  });
                }}
              />
            </div>
          </div>
        </div>

        {#if selectedAnalysisFailed || selectedAnalysisInProgress || !selectedAnalysisComplete}
          <div class="mt-6 border-b border-workspace-border/70 pb-4">
            {#if selectedAnalysisFailed}
            <div class="space-y-2">
              {#each selectedAnalysis.warnings as warning (`${warning.code ?? 'warn'}-${warning.message}`)}
                <InlineMessage tone="error" message={warning.message} />
              {:else}
                <InlineMessage tone="error">
                  Question analysis failed. Open Settings Diagnostics trace history for request and error details, then fix your LLM settings and use Re Analyze.
                </InlineMessage>
              {/each}
            </div>
            {:else if selectedAnalysisInProgress && !selectedAnalysisComplete}
            <div class="inline-flex items-center gap-2 shell-body text-workspace-text-secondary">
              <HugeiconsIcon icon={Loading03Icon} class="h-4 w-4 animate-spin" aria-hidden="true" />
              <span>Analyzing question...</span>
            </div>
            {:else if !selectedAnalysisComplete}
            <div class="shell-body text-workspace-text-secondary">
              Analysis has not completed yet. Question text edits stay disabled until cleaned text is ready.
            </div>
            {/if}
          </div>
        {/if}

<div class="mt-8">
          <div class="flex flex-wrap items-start gap-3">
            <div class="min-w-[15rem] flex-1 basis-[15rem]">
              <div class="flex items-center gap-2">
                <div id="review-question-text-heading" class="shell-body-lg font-semibold text-workspace-text-primary">
                  Question text
                </div>
                {#if selectedQuestionRecord.imagePath}
                  <IconButton
                    type="button"
                    class="h-6 w-6 rounded-md border-transparent bg-transparent"
                    variant="ghost"
                    size="compact"
                    ariaLabel="Toggle question image preview"
                    title="Toggle question image preview"
                    onclick={() => { imagePopoverOpen = !imagePopoverOpen; }}
                  >
                    <HugeiconsIcon icon={Image01Icon} class="h-4 w-4" aria-hidden="true" />
                  </IconButton>
                {/if}
              </div>
              <TextareaField
                controlClass="min-h-44 rounded-xl bg-surface-card-control px-5 py-4 text-lg"
                aria-labelledby="review-question-text-heading"
                value={selectedQuestionDraft.text}
                disabled={!selectedAnalysisComplete}
                oninput={(event: Event) =>
                  onUpdateQuestion?.(selectedQuestionDraft.questionId, {
                    text: (event.currentTarget as HTMLTextAreaElement).value
                  })}
              />
            </div>
            <div class="min-w-[15rem] flex-1 basis-[15rem]">
              <div class="flex items-center gap-2">
                <div id="review-question-context-heading" class="shell-body-lg font-semibold text-workspace-text-primary">
                  Question context
                </div>
                <DesktopPopover
                  bind:open={questionContextHelpOpen}
                  rootClass="relative inline-flex"
                  triggerClass="inline-flex h-6 w-6 items-center justify-center rounded-full border border-transparent text-workspace-text-muted transition-colors hover:text-workspace-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                  triggerLabel="Question context guidance"
                  triggerAriaHaspopup="dialog"
                  panelRole="dialog"
                  panelAriaLabel="Question context guidance"
                  panelClass="w-72 p-3"
                  aria-label="Question context guidance"
                >
                  <svelte:fragment slot="trigger">
                    <HugeiconsIcon
                      icon={InformationCircleIcon}
                      size={16}
                      strokeWidth={1.7}
                      aria-hidden="true"
                    />
                  </svelte:fragment>
                  <p class="text-sm leading-5 text-workspace-text-secondary">
                    Natural-language context visible in the image but not already in the cleaned question text, such as follow-up dependence, referenced figures, table meaning, or setup assumptions.
                  </p>
                </DesktopPopover>
              </div>
              {#if !selectedAnalysisComplete}
                <div class="mt-3 shell-meta text-workspace-text-muted">
                  Editable after analysis completes.
                </div>
              {:else}
                <TextareaField
                  controlClass="min-h-44 rounded-xl bg-surface-card-control px-5 py-4 text-lg"
                  aria-labelledby="review-question-context-heading"
                  value={selectedQuestionDraft.questionContext}
                  oninput={(event: Event) =>
                    patchQuestionContext(
                      selectedQuestionDraft.questionId,
                      (event.currentTarget as HTMLTextAreaElement).value
                    )}
                />
              {/if}
            </div>
          </div>

	          {#if imagePopoverOpen && selectedQuestionRecord.imagePath}
	            <div class="mt-4 w-full">
	              <PagePreviewFrame
	                src={toDesktopAssetUrl(selectedQuestionRecord.imagePath)}
	                alt={`Question ${selectedQuestionDraft.questionNumber} preview`}
	                class="max-w-full"
	                imageClass="block h-auto max-w-full select-none"
	              />
	            </div>
	          {/if}
        </div>

        <div class="mt-8">
          {#if pointsMismatchMessage}
            <InlineMessage
              class={`mb-2 inline-flex items-center gap-2 transition-opacity duration-300 ${pointsMismatchFlash ? 'animate-pulse opacity-100' : 'opacity-90'}`}
              tone="warning"
            >
              <HugeiconsIcon icon={AlertCircleIcon} size={16} strokeWidth={1.7} aria-hidden="true" />
              {pointsMismatchMessage}
            </InlineMessage>
          {/if}

          <div class="grid min-h-[24rem] overflow-hidden rounded-2xl border border-workspace-border bg-surface-card shadow-[var(--surface-shadow-card)] lg:grid-cols-[22rem_minmax(0,1fr)]">
            <aside
              class="flex min-h-0 flex-col border-b border-workspace-border bg-surface-sidebar lg:border-r lg:border-b-0"
              aria-label="Rubric criteria"
            >
              <div class="flex items-center justify-between gap-2 border-b border-workspace-border px-4 py-3">
                <div class="flex min-w-0 items-center gap-2">
                  <div class="truncate shell-body-lg font-semibold text-workspace-text-primary">
                    Rubric Criteria
                  </div>
                  <DesktopPopover
                    bind:open={rubricHelpOpen}
                    rootClass="relative inline-flex shrink-0"
                    triggerClass="inline-flex h-6 w-6 items-center justify-center rounded-full border border-transparent text-workspace-text-muted transition-colors hover:text-workspace-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                    triggerLabel="Rubric criteria help"
                    triggerAriaHaspopup="dialog"
                    panelRole="dialog"
                    panelAriaLabel="Rubric criteria help"
                    panelClass="w-72 p-3"
                    aria-label="Rubric criteria help"
                  >
                    <svelte:fragment slot="trigger">
                      <HugeiconsIcon
                        icon={InformationCircleIcon}
                        size={16}
                        strokeWidth={1.7}
                        aria-hidden="true"
                      />
                    </svelte:fragment>
                    <p class="text-sm leading-5 text-workspace-text-secondary">
                      Edits stay in memory until you save; the project database stores approved and draft rubrics.
                    </p>
                  </DesktopPopover>
                </div>
                <DesktopButton
                  class="shrink-0"
                  size="compact"
                  disabled={selectedRubricGenerationActive}
                  onclick={addCriterion}
                >
                  + add criterion
                </DesktopButton>
              </div>
              <nav class="min-h-0 flex-1 overflow-y-auto" aria-label="Criterion list">
                {#each $editableCriteria as criterion, index (criterion.criterionId || index)}
                  {@const isSelectedCriterion = index === selectedCriterionIndex}
                  {@const criterionWarnings = warningsForCriterionIndex(index)}
                  {@const displayLabel = criterionDisplayLabel(criterion, index)}
                  <div
                    class={[
                      'group relative flex w-full items-start px-4 py-3 transition-colors',
                      isSelectedCriterion
                        ? 'bg-workspace-sidebar-active text-workspace-text-primary'
                        : 'text-workspace-text-secondary hover:bg-workspace-sidebar-hover'
                    ]}
                  >
                    <div
                      class={[
                        'absolute inset-y-0 left-0 w-[3px] rounded-r',
                        isSelectedCriterion ? 'bg-primary' : 'bg-transparent'
                      ]}
                    ></div>

                    <button
                      type="button"
                      class="min-w-0 flex-1 text-left"
                      aria-pressed={isSelectedCriterion}
                      title={displayLabel}
                      onclick={() => selectCriterion(index)}
                    >
                      <div class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-x-2 gap-y-1 pr-1">
                        <span
                          class={`line-clamp-2 min-h-10 text-sm font-medium leading-5 ${isSelectedCriterion ? 'text-workspace-text-primary' : 'text-workspace-text-secondary'}`}
                        >
                          {displayLabel}
                        </span>
                        <span class="flex min-h-5 items-start justify-end gap-1">
                          {#if criterion.source === 'minimum_credit'}
                            <span
                              class="inline-flex shrink-0 text-workspace-text-muted"
                              aria-label="Minimum credit criterion"
                              title="Minimum credit criterion"
                            >
                              <HugeiconsIcon
                                icon={PercentCircleIcon}
                                size={17}
                                strokeWidth={1.7}
                                aria-hidden="true"
                              />
                            </span>
                          {/if}
                          {#if criterionWarnings.length > 0}
                            <span
                              class="inline-flex shrink-0 text-message-warning-text"
                              title={criterionWarningTitle(criterionWarnings)}
                            >
                              <ToneIcon
                                tone="warning"
                                icon={ApproximatelyEqualIcon}
                                label="Criterion warning"
                                title={criterionWarningTitle(criterionWarnings)}
                              />
                            </span>
                          {/if}
                        </span>
                        <span class="shell-meta tabular-nums text-workspace-text-muted">
                          {criterionPointsLabel(criterion.points)}
                        </span>
                      </div>
                    </button>
                  </div>
                  <div class="h-px bg-workspace-border"></div>
                {:else}
                  <div class="px-4 py-6 shell-body text-workspace-text-secondary">
                    No criteria yet.
                  </div>
                {/each}
              </nav>
            </aside>

            <div class="min-w-0 p-4 sm:p-5">
              {#if selectedCriterion}
                <div class="flex items-start justify-between gap-4">
                  <div class="min-w-0 flex-1">
                    <TextField
                      label="Label"
                      controlClass="min-h-12 rounded-xl bg-surface-card-control text-base font-medium"
                      value={selectedCriterion.label}
                      placeholder="Criterion label"
                      oninput={(event: Event) =>
                        updateCriterion(selectedCriterionIndex, {
                          label: (event.currentTarget as HTMLInputElement).value
                        })}
                    />
                  </div>
                  <IconButton
                    variant="danger"
                    class="size-14 rounded-2xl"
                    ariaLabel="Remove criterion"
                    title="Remove criterion"
                    onclick={() => requestRemoveCriterion(selectedCriterionIndex)}
                  >
                    <HugeiconsIcon icon={Delete02Icon} size={28} strokeWidth={1.8} aria-hidden="true" />
                  </IconButton>
                </div>

                {#if selectedCriterionWarnings.length > 0}
                  <div class="mt-4 space-y-2">
                    {#each selectedCriterionWarnings as warning (`${warning.code ?? 'warn'}-${warning.message}`)}
                      <InlineMessage tone="warning" message={warning.message} />
                    {/each}
                  </div>
                {/if}

                <div class="mt-5 grid gap-5">
                  <TextareaField
                    label="Partial Credit Guidance"
                    controlClass="min-h-36 rounded-xl bg-surface-card-control px-4 py-3 text-sm"
                    placeholder="Partial credit guidance"
                    value={selectedCriterion.partialCreditGuidance}
                    oninput={(event: Event) =>
                      updateCriterion(selectedCriterionIndex, {
                        partialCreditGuidance: (event.currentTarget as HTMLTextAreaElement).value
                      })}
                  />

                  <div>
                    <div class="shell-meta font-medium text-workspace-text-muted">Points</div>
                    <div class="mt-2 flex flex-wrap gap-1">
                      {#each selectedCriterionPointOptions as pointValue (pointValue)}
                        <button
                          type="button"
                          class={`inline-flex h-8 w-8 items-center justify-center rounded-md border text-sm tabular-nums transition-colors ${
                            pointValue === selectedCriterion.points
                              ? 'border-message-success-border bg-message-success-bg text-message-success-text'
                              : 'border-border bg-surface-card-control text-workspace-text-muted hover:bg-muted/40'
                          }`}
                          aria-label={`${pointValue} point${pointValue !== 1 ? 's' : ''}`}
                          title={`${pointValue} point${pointValue !== 1 ? 's' : ''}`}
                          aria-pressed={pointValue === selectedCriterion.points}
                          onclick={() => updateCriterion(selectedCriterionIndex, { points: pointValue })}
                        >
                          {pointValue}
                        </button>
                      {/each}
                    </div>
                  </div>
                </div>
              {:else}
                <div class="flex min-h-[18rem] items-center justify-center rounded-xl border border-dashed border-workspace-border bg-surface-panel px-6 text-center shell-body text-workspace-text-secondary">
                  No criteria yet.
                </div>
              {/if}
            </div>
          </div>
        </div>
      </div>

      <div class="flex items-center justify-between gap-4 border-t border-workspace-border bg-surface-bottom-bar px-8 py-5">
        <div class="shell-body text-workspace-text-secondary">
          Question {selectedQuestionDraft.questionNumber} of {orderedQuestionDrafts.length} (Page {selectedQuestionDraft.pageNumber})
        </div>
        <div class="flex items-center gap-3">
          {#if !selectedRubricApproved}
            <DesktopButton
              size="large"
              disabled={busyAction !== null || selectedAnalysisInProgress || hasUnsavedQuestionEdits}
              onclick={() => void onReAnalyze?.(selectedQuestionDraft.questionId)}
            >
              {selectedAnalysisInProgress ? 'Re-analyzing…' : 'Re Analyze'}
            </DesktopButton>
            <DesktopButton
              size="large"
              disabled={
                busyAction === 'generateRubric' ||
                selectedRubricGenerationActive ||
                !selectedAnalysisComplete
              }
              onclick={() => void onGenerateRubric?.(selectedQuestionDraft.questionId)}
            >
              <HugeiconsIcon icon={AiMagicIcon} size={18} strokeWidth={1.8} aria-hidden="true" />
              {busyAction === 'generateRubric' ? 'Generating...' : 'Generate rubric'}
            </DesktopButton>
          {/if}
          {#if selectedRubricApproved}
            <DesktopButton
              size="large"
              class="text-destructive hover:text-destructive"
              disabled={
                busyAction !== null ||
                !onSaveReviewChanges ||
                hasUnsavedReviewChanges ||
                !workspaceState.canApproveRubric
              }
              onclick={rescindApprovedRubric}
            >
              Rescind approval
            </DesktopButton>
          {/if}
          <DesktopButton
            size="large"
            disabled={
              busyAction !== null ||
              !onSaveReviewChanges ||
              !hasUnsavedReviewChanges ||
              (hasUnsavedRubricEdits && !workspaceState.canApproveRubric)
            }
            onclick={requestSaveReviewChanges}
          >
            {busyAction === 'saveQuestions' || busyAction === 'saveRubric' ? 'Saving...' : 'Save'}
          </DesktopButton>
          {#if !selectedRubricApproved}
            <DesktopButton
              size="large"
              variant="primary"
              disabled={
                busyAction !== null || !workspaceState.canApproveRubric || hasUnsavedQuestionEdits
              }
              onclick={() => void approveRubricAndAdvance()}
            >
              Approve rubric
            </DesktopButton>
          {/if}
        </div>
      </div>
    {:else}
      <div class="flex h-full items-center justify-center px-8 shell-body-lg text-workspace-text-secondary">
        No questions are available for review yet.
      </div>
    {/if}
  </section>

  <ConfirmDialog
    open={pendingDeleteCriterionIndex !== null}
    title="Delete criterion?"
    description={pendingDeleteCriterion && pendingDeleteCriterionIndex != null
      ? `This removes "${criterionDisplayLabel(pendingDeleteCriterion, pendingDeleteCriterionIndex)}" from the rubric draft.`
      : 'This removes the selected criterion from the rubric draft.'}
    confirmLabel="Delete criterion"
    destructive
    onCancel={cancelRemoveCriterion}
    onConfirm={confirmRemoveCriterion}
  />

  {#if approvedSaveDecisionOpen && selectedQuestionDraft}
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-overlay-scrim px-4">
      <div
        class="w-full max-w-lg rounded-2xl border border-border-default bg-surface-canvas p-5 shadow-[var(--surface-shadow-strong)]"
        role="dialog"
        aria-modal="true"
        aria-labelledby="approved-save-title"
        aria-describedby="approved-save-description"
      >
        <div id="approved-save-title" class="text-lg font-semibold text-text-primary">
          Save approved rubric changes?
        </div>
        <p id="approved-save-description" class="mt-2 text-base leading-7 text-text-secondary">
          This question already has an approved rubric. Choose how these edits should affect rubric approval and downstream grading.
        </p>
        {#if selectedQuestionBasisChanged || hasUnsavedRubricEdits}
          <div class="mt-4 flex items-start gap-2 text-sm leading-6 text-text-secondary">
            <HugeiconsIcon
              icon={InformationCircleIcon}
              size={17}
              strokeWidth={1.7}
              class="mt-0.5 shrink-0 text-workspace-text-muted"
              aria-hidden="true"
            />
            {#if selectedRubricStructuralChange}
              <span>Criteria were added, removed, or had point values changed. Minor save is unavailable for this edit.</span>
            {:else if selectedQuestionDraft.maxPoints !== selectedQuestionRecord?.maxPoints}
              <span>Question max points changed. Minor save is unavailable for this edit.</span>
            {:else}
              <span>Only question text, context, or rubric wording changed.</span>
            {/if}
          </div>
        {/if}
        <div class="mt-5 grid gap-2">
          <DesktopButton size="large" onclick={discardApprovedReviewChanges}>
            Discard changes
          </DesktopButton>
          <DesktopButton
            size="large"
            disabled={!minorApprovedSaveAllowed || busyAction !== null}
            onclick={() => saveApprovedReviewChanges('minor')}
          >
            Save as minor edit
          </DesktopButton>
          <DesktopButton
            size="large"
            variant="destructive"
            disabled={busyAction !== null}
            onclick={() => saveApprovedReviewChanges('grading')}
          >
            Save and rescind approval
          </DesktopButton>
        </div>
      </div>
    </div>
  {/if}

</div>
