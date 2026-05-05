<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { writable } from 'svelte/store';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import {
    AlertCircleIcon,
    Cancel01Icon,
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
    DesktopButton,
    IconButton,
    InlineMessage,
    PagePreviewFrame,
    Surface,
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

  const editableCriteria = writable<RubricCriterion[]>([]);

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

  function updateCriterion(index: number, patch: Partial<RubricCriterion>) {
    editableCriteria.update((criteria) =>
      criteria.map((criterion, currentIndex) =>
        currentIndex === index ? { ...criterion, ...patch } : criterion
      )
    );
  }

  function addCriterion() {
    editableCriteria.update((criteria) => [
      ...criteria,
      {
        criterionId: '',
        label: '',
        points: 1,
        partialCreditGuidance: '',
        source: 'manual'
      }
    ]);
  }

  function removeCriterion(index: number) {
    editableCriteria.update((criteria) => criteria.filter((_, i) => i !== index));
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
                <div id="review-question-text-heading" class="shell-body font-medium text-workspace-text-secondary">
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
              <div id="review-question-context-heading" class="shell-body font-medium text-workspace-text-secondary" title="Natural-language context visible in the image but not already in the cleaned question text (e.g. follow-up dependence, referenced figures, table meaning, setup assumptions).">Question context</div>
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
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div>
              <div class="shell-body font-medium text-workspace-text-secondary">Rubric criteria</div>
              <div class="mt-1 shell-meta text-workspace-text-muted">Edits stay in memory until you save; the project database stores approved and draft rubrics.</div>
              {#if pointsMismatchMessage}
                <InlineMessage
                  class={`mt-2 inline-flex items-center gap-2 transition-opacity duration-300 ${pointsMismatchFlash ? 'animate-pulse opacity-100' : 'opacity-90'}`}
                  tone="warning"
                >
                  <HugeiconsIcon icon={AlertCircleIcon} size={16} strokeWidth={1.7} aria-hidden="true" />
                  {pointsMismatchMessage}
                </InlineMessage>
              {/if}
            </div>
            <div class="flex flex-wrap items-center gap-2">
              <DesktopButton
                size="large"
                disabled={
                  busyAction === 'generateRubric' ||
                  selectedRubricGenerationActive ||
                  !selectedAnalysisComplete
                }
                onclick={() => void onGenerateRubric?.(selectedQuestionDraft.questionId)}
              >
                {busyAction === 'generateRubric' ? 'Generating…' : 'Generate rubric'}
              </DesktopButton>
              <DesktopButton
                size="large"
                disabled={selectedRubricGenerationActive}
                onclick={addCriterion}
              >
                + add criterion
              </DesktopButton>
            </div>
          </div>
          <div class="mt-4 space-y-2">
            {#each $editableCriteria as criterion, index (criterion.criterionId || index)}
              {@const criterionWarnings = warningsForCriterionIndex(index)}
              <Surface
                variant={index % 2 === 1 ? 'cardSubtle' : 'panel'}
                bordered={criterionWarnings.length > 0}
                radius="2xl"
                class={criterionWarnings.length > 0 ? 'border-message-warning-border px-3 py-4' : 'px-3 py-4'}
              >
                <div class="flex flex-col gap-2">
                  <div class="flex flex-wrap items-center gap-3">
                    <TextField
                      class="min-w-0 flex-1"
                      controlClass="min-h-10 rounded-xl bg-surface-card-control"
                      value={criterion.label}
                      placeholder="Criterion label"
                      oninput={(event: Event) => updateCriterion(index, { label: (event.currentTarget as HTMLInputElement).value })}
                    />
                    {#if criterionWarnings.length > 0}
                      <span
                        class="inline-flex shrink-0 text-message-warning-text"
                        title={criterionWarnings.map(w => w.message).join('\n')}
                      >
                        <HugeiconsIcon icon={AlertCircleIcon} size={16} strokeWidth={1.7} aria-hidden="true" />
                      </span>
                    {/if}
                    {#if criterion.source === 'minimum_credit'}
                      <span class="inline-flex shrink-0" title="Minimum credit criterion">
                        <HugeiconsIcon
                          icon={PercentCircleIcon}
                          size={20}
                          strokeWidth={1.7}
                          class="text-workspace-text-muted"
                        />
                      </span>
                    {/if}
                    <label class="flex items-center gap-2 shell-meta text-workspace-text-muted">
                      <span>Pts</span>
                      <TextField
                        class="w-16"
                        controlClass="h-10 rounded-xl bg-surface-card-control text-center [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                        type="number"
                        min="0"
                        title="Points"
                        value={criterion.points}
                        oninput={(event: Event) =>
                          updateCriterion(index, { points: Number.parseInt((event.currentTarget as HTMLInputElement).value, 10) || 0 })}
                      />
                    </label>
                    <DesktopButton
                      variant="ghost"
                      size="compact"
                      class="shrink-0 text-workspace-text-muted hover:text-destructive"
                      title="Remove criterion"
                      aria-label="Remove criterion"
                      onclick={() => removeCriterion(index)}
                    >
                      Remove
                    </DesktopButton>
                  </div>
                  <TextareaField
                    controlClass="min-h-20 rounded-xl bg-surface-card-control px-4 py-3 text-sm"
                    placeholder="Partial credit guidance"
                    value={criterion.partialCreditGuidance}
                    oninput={(event: Event) =>
                      updateCriterion(index, {
                        partialCreditGuidance: (event.currentTarget as HTMLTextAreaElement).value
                      })}
                  />
                </div>
              </Surface>
            {:else}
              <div class="py-6 text-sm text-workspace-text-secondary">No criteria yet.</div>
            {/each}
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
