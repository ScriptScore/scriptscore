<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { toDesktopAssetUrl } from '$lib/desktop';
  import { Delete02Icon } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import type { StudentWorkflowSubmission, StudentWorkflowAnswer } from '$lib/types';
  import HorizontalProgressBar from './HorizontalProgressBar.svelte';
  import {
    stageLabel,
    stageProgressTone,
    confidenceLabel,
    confidenceBadgeTone,
    highlightKindToClass,
    markedTextSegments
  } from './student-workflow-helpers';
  import { DesktopButton, IconButton, InlineMessage, PagePreviewFrame, StatusBadge } from './ui';

  export let submission: StudentWorkflowSubmission | null;
  export let displayName: string;
  export let busyAction: BusyAction = null;
  export let reviewSaveBusy: Record<string, boolean> = {};
  export let stageProgress = 0;
  export let onconfirmparsereview:
    | ((studentRef: string, questionId: string, correctedText: string) => Promise<void>)
    | null = null;
  export let onsavecriterionscore:
    | ((studentRef: string, questionId: string, criterionIndex: number, pointsAwarded: number) => Promise<void>)
    | null = null;
  export let ondelete: (() => void) | null = null;
  export let deleteDisabled = false;
  export let onback: () => void;

  let selectedQuestionId: string | null = null;
  let parseDraftText = '';
  let questionDraftKey: string | null = null;

  $: stage = submission?.stage ?? null;
  $: stageText = stageLabel(stage ?? '');
  $: stageTone = stageProgressTone(stage ?? '');
  $: stageActive = ['alignment', 'canonicalize', 'transform', 'detect', 'crop', 'pii', 'parse', 'grading'].includes(stage ?? '');
  function answerSortKey(answer: StudentWorkflowAnswer): string {
    return `${String(answer.questionNumber).padStart(6, '0')}:${answer.questionId}`;
  }

  $: answers = [...(submission?.answers ?? [])].sort((left, right) =>
    answerSortKey(left).localeCompare(answerSortKey(right))
  );
  $: reviewRequiredAnswers = answers.filter((a) => a.reviewRequired);
  $: isParseReview = stage === 'parse_review';
  $: isManualGrading = stage === 'manual_grading';
  $: actionableAnswers = isParseReview
    ? reviewRequiredAnswers
    : isManualGrading
      ? answers.filter((a) => a.manualGradingRequired === true)
      : [];
  $: isGraded = stage === 'graded';
  $: globalControlsDisabled = busyAction !== null && busyAction !== 'studentWorkflow';

  function isConfident(answer: StudentWorkflowAnswer): boolean {
    if (isParseReview) {
      return answer.parseConfidence === 'high' || answer.parseConfidence === 'medium';
    }
    if (answer.gradingStatus === 'draft_ready' || isGraded) {
      return answer.gradingConfidence === 'high' || answer.gradingConfidence === 'medium';
    }
    return false;
  }

  function handwritingLabel(state: string | null | undefined): string {
    switch (state) {
      case 'true':
        return 'handwriting detected';
      case 'false':
        return 'no handwriting detected';
      default:
        return 'handwriting inconclusive';
    }
  }

  function piiBlockMessage(answer: StudentWorkflowAnswer): string {
    switch (answer.manualGradingReason) {
      case 'pii_detected':
        return 'Private student information was detected in this answer, so automatic grading and feedback were skipped.';
      case 'crop_failed':
        return 'ScriptScore could not isolate this answer clearly, so it needs manual grading.';
      default:
        return 'This answer needs manual review because the privacy check was not clean.';
    }
  }

  function manualBlockHeading(answer: StudentWorkflowAnswer): string {
    return answer.manualGradingReason === 'crop_failed' ? 'Answer image result' : 'Privacy check result';
  }

  function manualBlockMeta(answer: StudentWorkflowAnswer): string {
    if (answer.manualGradingReason === 'crop_failed') {
      return 'No privacy check was recorded because the answer image could not be prepared cleanly.';
    }
    return handwritingLabel(answer.piiPrescreen?.containsHandwriting);
  }

  function manualBlockDetail(answer: StudentWorkflowAnswer): string {
    if (answer.manualGradingReason === 'crop_failed') {
      return 'The answer image could not be prepared, so this row must be handled manually.';
    }
    return (answer.piiPrescreen?.piiTypesDetected?.length ?? 0) > 0
      ? `Detected private information: ${answer.piiPrescreen?.piiTypesDetected?.join(', ')}`
      : 'No specific category was recorded, but the privacy check still needs your review.';
  }

  function criterionDotClass(pointsAwarded: number, points: number): string {
    if (pointsAwarded <= 0) return 'bg-destructive';
    if (points > 0 && pointsAwarded >= points) return 'bg-message-success-border';
    return 'bg-message-warning-border';
  }

  function saveBusyKey(kind: string, ...parts: Array<string | number>): string {
    return [kind, ...parts].join(':');
  }

  function parseSaving(answer: StudentWorkflowAnswer | null): boolean {
    return (
      !!submission &&
      !!answer &&
      reviewSaveBusy[saveBusyKey('parse', submission.studentRef, answer.questionId)] === true
    );
  }

  function criterionSaving(answer: StudentWorkflowAnswer): boolean {
    return (
      !!submission &&
      reviewSaveBusy[saveBusyKey('criterion', submission.studentRef, answer.questionId)] === true
    );
  }

  $: selectedAnswer =
    answers.find((a) => a.questionId === selectedQuestionId) ?? answers[0] ?? null;
  $: selectedCropSrc = selectedAnswer?.cropImagePath
    ? toDesktopAssetUrl(selectedAnswer.cropImagePath)
    : '';

  $: {
    const nextId = answers[0]?.questionId ?? null;
    const hasCurrent = answers.some((a) => a.questionId === selectedQuestionId);
    if (!hasCurrent) {
      selectedQuestionId = nextId;
    }
  }

  $: {
    const key = submission
      ? `${submission.studentRef}:${submission.latestJobId ?? ''}:${stage ?? ''}:${actionableAnswers
          .map((a) =>
            [
              a.questionId,
              a.reviewRequired,
              a.manualGradingRequired === true,
              a.parseStatus,
              a.gradingStatus
            ].join(':')
          )
          .join(',')}`
      : null;
    if (key && key !== questionDraftKey) {
      const first = actionableAnswers[0] ?? answers[0] ?? null;
      selectedQuestionId = first?.questionId ?? null;
      if (isParseReview && first?.reviewRequired) {
        parseDraftText = first.verifiedText ?? first.rawParsedText ?? '';
      }
      questionDraftKey = key;
    } else if (parseDraftText.trim().length === 0) {
      const current =
        reviewRequiredAnswers.find((a) => a.questionId === selectedQuestionId) ??
        reviewRequiredAnswers[0] ??
        null;
      if (current) {
        parseDraftText = current.verifiedText ?? current.rawParsedText ?? '';
      }
    }
  }

  async function handleConfirmParseReview() {
    if (!submission || !selectedAnswer) return;
    await onconfirmparsereview?.(
      submission.studentRef,
      selectedAnswer.questionId,
      parseDraftText
    );
  }

  const cropPreviewImageClass = 'block max-h-[42vh] max-w-full object-contain';
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="flex shrink-0 items-center px-6 pt-0 pb-2">
    <div class="w-40">
      <DesktopButton class="whitespace-nowrap" variant="ghost" size="compact" onclick={onback}>Back to workflow</DesktopButton>
    </div>
    <div class="flex min-w-0 flex-1 items-center justify-center">
      <div class="min-w-0 max-w-[60%] truncate text-sm font-semibold text-workspace-text-primary">
        {displayName}
      </div>
    </div>
    <div class="flex w-56 items-center justify-end gap-2">
      <HorizontalProgressBar
        label={stageText}
        progress={stageProgress}
        tone={stageTone}
        active={stageActive}
        complete={stage === 'graded'}
        showPercent={stageActive}
        className="w-44"
      />
      {#if ondelete}
        <IconButton
          variant="danger"
          size="compact"
          ariaLabel="Delete submission"
          title={`Delete submission for ${displayName}`}
          disabled={deleteDisabled}
          onclick={() => ondelete?.()}
        >
          <HugeiconsIcon icon={Delete02Icon} size={17} strokeWidth={1.8} aria-hidden="true" />
        </IconButton>
      {/if}
    </div>
  </div>

  {#if answers.length > 0}
    <div class="relative shrink-0 border-b border-workspace-border bg-workspace-sidebar">
      <div class="flex overflow-x-auto px-6">
        {#each answers as answer (answer.questionId)}
          {@const isActive = answer.questionId === selectedAnswer?.questionId}
          {@const needsReview = answer.reviewRequired && isParseReview}
          {@const manualRequired = answer.manualGradingRequired ?? false}
          {@const confident = isConfident(answer)}
          <button
            type="button"
            class={[
              'relative flex shrink-0 items-center gap-1.5 px-4 pb-2.5 pt-2 text-sm transition-colors',
              isActive
                ? 'text-workspace-text-primary'
                : 'text-workspace-text-muted hover:text-workspace-text-secondary'
            ]}
            onclick={() => {
              selectedQuestionId = answer.questionId;
              if (answer.reviewRequired && isParseReview) {
                parseDraftText = answer.verifiedText ?? answer.rawParsedText ?? '';
              }
            }}
          >
            <span
              class="absolute inset-x-0 bottom-0 h-[3px] rounded-t"
              class:bg-primary={isActive}
              class:bg-transparent={!isActive}
            ></span>
            {#if manualRequired}
              <span class="font-semibold text-message-warning-text">Q{answer.questionNumber}</span>
              <span class="text-[10px] font-semibold uppercase tracking-wider text-message-warning-text">manual</span>
            {:else if needsReview}
              <span class="font-semibold text-message-warning-text">Q{answer.questionNumber}</span>
              <span class="text-[10px] font-semibold uppercase tracking-wider text-message-warning-text">review</span>
            {:else if confident}
              <span class="font-semibold">Q{answer.questionNumber}</span>
              <span class="text-[10px] text-workspace-text-muted">✓</span>
            {:else}
              <span class="font-medium">Q{answer.questionNumber}</span>
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto px-6 pt-4 pb-6">
    {#if submission?.failureMessage}
      <InlineMessage tone="error" class="mb-3 rounded-2xl px-5 py-4">
        {submission.failureMessage}
      </InlineMessage>
    {/if}

    {#if selectedAnswer}
      {@const needsReview = selectedAnswer.reviewRequired && isParseReview}
      {@const manualRequired = selectedAnswer.manualGradingRequired ?? false}
      {@const answerText = selectedAnswer.verifiedText ?? selectedAnswer.rawParsedText ?? '[blank]'}
      {@const hasHighlights = !needsReview && (selectedAnswer.highlights?.length ?? 0) > 0}

      {#if manualRequired}
        <InlineMessage tone="warning" class="mb-4 rounded-2xl px-5 py-4">
          {piiBlockMessage(selectedAnswer)}
        </InlineMessage>
      {:else if needsReview}
        <InlineMessage tone="warning" class="mb-4 rounded-2xl px-5 py-4">
          {confidenceLabel(selectedAnswer.parseConfidence)} recognized text needs review before
          grading can continue.
        </InlineMessage>
      {/if}

      <div class="space-y-5">
        {#if needsReview}
          <div class="grid grid-cols-1 gap-6 xl:grid-cols-[24rem_minmax(0,1fr)]">
            <div class="min-w-0 space-y-4">
              {#if selectedCropSrc}
                <PagePreviewFrame
                  class="flex w-full items-center justify-center p-2"
                  src={selectedCropSrc}
                  alt="Question {selectedAnswer.questionNumber} crop"
                  imageClass={cropPreviewImageClass}
                />
              {/if}

              <div>
                <div
                  class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted"
                >
                  Corrected answer
                  <StatusBadge
                    tone={confidenceBadgeTone(selectedAnswer.parseConfidence)}
                    class="ml-2 min-h-0 px-2 py-0.5 text-[10px]"
                  >
                    {confidenceLabel(selectedAnswer.parseConfidence)}
                  </StatusBadge>
                </div>
                <textarea
                  class="mt-2 min-h-[16rem] w-full rounded-2xl border border-workspace-border bg-surface-card-control px-4 py-3 text-sm text-workspace-text-primary"
                  bind:value={parseDraftText}
                ></textarea>
                <div class="mt-2 text-xs text-workspace-text-muted">
                  The corrected answer becomes the grading input. The original answer image and raw
                  recognized text remain preserved.
                </div>
                <div class="mt-4 flex justify-end">
                  <DesktopButton
                    variant="secondary"
                    disabled={globalControlsDisabled || parseSaving(selectedAnswer)}
                    onclick={() => void handleConfirmParseReview()}
                  >
                    {parseSaving(selectedAnswer) ? 'Saving…' : 'Confirm answer → continue'}
                  </DesktopButton>
                </div>
              </div>

              {#if selectedAnswer.rawParsedText}
                <div>
                  <div
                    class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted"
                  >
                    Raw recognized text
                  </div>
                  <div
                    class="mt-2 whitespace-pre-wrap rounded-2xl border border-workspace-border bg-surface-card-control px-4 py-3 text-xs font-mono text-workspace-text-secondary"
                  >
                    {selectedAnswer.rawParsedText}
                  </div>
                </div>
              {/if}
            </div>

            <div class="min-w-0"></div>
          </div>
        {:else if manualRequired}
          <div class="grid grid-cols-1 gap-8 xl:grid-cols-[27rem_minmax(0,1fr)]">
            <div class="min-w-0">
              {#if selectedCropSrc}
                <PagePreviewFrame
                  class="flex w-full items-center justify-center p-2"
                  src={selectedCropSrc}
                  alt="Question {selectedAnswer.questionNumber} crop"
                  imageClass={cropPreviewImageClass}
                />
              {/if}
            </div>

            <div class="min-w-0 space-y-4">
              <div>
                <div class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">
                  {manualBlockHeading(selectedAnswer)}
                </div>
                <div class="mt-2 rounded-2xl border border-workspace-border bg-surface-card-control px-4 py-4 text-sm text-workspace-text-primary">
                  <div>{piiBlockMessage(selectedAnswer)}</div>
                  <div class="mt-3 text-xs text-workspace-text-secondary">
                    {manualBlockMeta(selectedAnswer)}
                  </div>
                  <div class="mt-2 text-xs text-workspace-text-secondary">
                    {manualBlockDetail(selectedAnswer)}
                  </div>
                </div>
              </div>

              {#if selectedAnswer.warnings.length > 0}
                <div>
                  <div class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">
                    Warnings
                  </div>
                  <div class="mt-2 space-y-2">
                    {#each selectedAnswer.warnings as warning (`${warning.code ?? 'warning'}:${warning.message}`)}
                      <InlineMessage tone="muted" class="rounded-2xl px-4 py-3">
                        {warning.message}
                      </InlineMessage>
                    {/each}
                  </div>
                </div>
              {/if}

              {#if selectedAnswer.criterionResults.length > 0}
                <div>
                  <div class="flex items-baseline justify-between">
                    <div
                      class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted"
                    >
                      Rubric scoring
                    </div>
                    <div class="text-lg font-semibold text-workspace-text-primary">
                      {selectedAnswer.totalPointsAwarded ?? 0}
                      {#if selectedAnswer.questionMaxPoints !== null}
                        <span class="text-sm font-normal text-workspace-text-muted">/ {selectedAnswer.questionMaxPoints}</span>
                      {/if}
                    </div>
                  </div>
                  <div class="mt-2 divide-y divide-workspace-border rounded-2xl border border-workspace-border">
                    {#each selectedAnswer.criterionResults as criterion (`manual:${criterion.criterionIndex}:${criterion.pointsAwarded}:${criterion.rationale}`)}
                      <div class="flex items-start gap-3 px-4 py-2.5">
                        <span
                          class={`mt-1 inline-block size-2 shrink-0 rounded-full ${criterionDotClass(
                            criterion.pointsAwarded,
                            criterion.points
                          )}`}
                        ></span>
                        <div class="min-w-0 flex-1">
                          <div class="text-sm font-medium text-workspace-text-primary">
                            {criterion.label || `Criterion ${criterion.criterionIndex + 1}`}
                          </div>
                          {#if criterion.rationale}
                            <div class="mt-0.5 whitespace-pre-wrap text-xs text-workspace-text-muted">
                              {criterion.rationale}
                            </div>
                          {/if}
                        </div>
                        {#if onsavecriterionscore}
                          <div class="flex shrink-0 flex-wrap gap-0.5">
                            {#each Array.from({ length: criterion.points + 1 }, (_, i) => i) as pointValue (pointValue)}
                              <button
                                type="button"
                                class={`inline-flex h-6 w-6 items-center justify-center rounded-md border text-xs tabular-nums transition-colors ${
                                  pointValue === criterion.pointsAwarded
                                    ? 'border-message-success-border bg-message-success-bg text-message-success-text'
                                    : 'border-border bg-surface-card-control text-workspace-text-muted hover:bg-muted/40'
                                } ${globalControlsDisabled || criterionSaving(selectedAnswer) ? 'pointer-events-none opacity-50' : ''}`}
                                title={`${pointValue} point${pointValue !== 1 ? 's' : ''}`}
                                disabled={globalControlsDisabled || criterionSaving(selectedAnswer)}
                                onclick={() => {
                                  if (pointValue !== criterion.pointsAwarded && submission) {
                                    void onsavecriterionscore?.(
                                      submission.studentRef,
                                      selectedAnswer.questionId,
                                      criterion.criterionIndex,
                                      pointValue
                                    );
                                  }
                                }}
                              >{pointValue}</button>
                            {/each}
                          </div>
                        {:else}
                          <div class="shrink-0 text-xs font-semibold tabular-nums text-workspace-text-muted">
                            {criterion.pointsAwarded}/{criterion.points}
                          </div>
                        {/if}
                      </div>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          </div>
        {:else}
          <div class="grid grid-cols-1 gap-8 xl:grid-cols-[27rem_minmax(0,1fr)]">
            <div class="min-w-0">
              {#if selectedCropSrc}
                <PagePreviewFrame
                  class="flex w-full items-center justify-center p-2"
                  src={selectedCropSrc}
                  alt="Question {selectedAnswer.questionNumber} crop"
                  imageClass={cropPreviewImageClass}
                />
              {/if}
            </div>

            <div class="min-w-0 space-y-4">
              <div>
                <div
                  class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted"
                >
                  Parsed answer
                  <StatusBadge
                    tone={confidenceBadgeTone(selectedAnswer.parseConfidence)}
                    class="ml-2 min-h-0 px-2 py-0.5 text-[10px]"
                  >
                    {confidenceLabel(selectedAnswer.parseConfidence)}
                  </StatusBadge>
                </div>
                {#if hasHighlights}
                  {@const sourceText = selectedAnswer.verifiedText ?? selectedAnswer.rawParsedText ?? ''}
                  <div
                    class="mt-2 whitespace-pre-wrap rounded-2xl border border-workspace-border bg-surface-card-control px-4 py-4 text-sm text-workspace-text-primary"
                  >
                    {#each markedTextSegments(sourceText, selectedAnswer.highlights ?? []) as segment, index (`${segment.kind ?? 'plain'}:${index}`)}
                      {#if segment.kind}
                        <span class={`${highlightKindToClass(segment.kind)} border rounded px-0.5`}>
                          {segment.text}
                        </span>
                      {:else}
                        {segment.text}
                      {/if}
                    {/each}
                  </div>
                {:else}
                  <div
                    class="mt-2 whitespace-pre-wrap rounded-2xl border border-workspace-border bg-surface-card-control px-4 py-4 text-sm text-workspace-text-primary"
                  >
                    {answerText}
                  </div>
                {/if}
              </div>

              {#if selectedAnswer.rawParsedText &&
                selectedAnswer.verifiedText &&
                selectedAnswer.rawParsedText !== selectedAnswer.verifiedText}
                <div>
                  <div
                    class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted"
                  >
                    Raw recognized text
                  </div>
                  <div
                    class="mt-2 whitespace-pre-wrap rounded-2xl border border-workspace-border bg-surface-card-control px-4 py-4 text-sm text-workspace-text-secondary"
                  >
                    {selectedAnswer.rawParsedText}
                  </div>
                </div>
              {/if}
            </div>

            {#if selectedAnswer.feedbackText || selectedAnswer.gradingConfidenceReason}
              <div class="min-w-0 xl:col-span-2">
                {#if selectedAnswer.feedbackText}
                  <div class="border-l-2 border-workspace-border-strong pl-4 whitespace-pre-wrap italic text-sm text-workspace-text-secondary">
                    {selectedAnswer.feedbackText}
                  </div>
                {/if}
                {#if selectedAnswer.gradingConfidenceReason}
                  <div class={`text-xs text-workspace-text-muted ${selectedAnswer.feedbackText ? 'mt-3 pl-4' : ''}`}>
                    {selectedAnswer.gradingConfidenceReason}
                  </div>
                {/if}
              </div>
            {/if}

            {#if selectedAnswer.criterionResults.length > 0}
              <div class="min-w-0 xl:col-span-2">
                <div class="flex items-baseline justify-between">
                  <div
                    class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted"
                  >
                    Criterion results
                    {#if selectedAnswer.gradingConfidence}
                      <StatusBadge
                        tone={confidenceBadgeTone(selectedAnswer.gradingConfidence)}
                        class="ml-2 min-h-0 px-2 py-0.5 text-[10px]"
                      >
                        {confidenceLabel(selectedAnswer.gradingConfidence)}
                      </StatusBadge>
                    {/if}
                  </div>
                  <div class="text-lg font-semibold text-workspace-text-primary">
                    {selectedAnswer.totalPointsAwarded ?? '—'}
                    {#if selectedAnswer.questionMaxPoints !== null}
                      <span class="text-sm font-normal text-workspace-text-muted">/ {selectedAnswer.questionMaxPoints}</span>
                    {/if}
                  </div>
                </div>
                <div class="mt-2 divide-y divide-workspace-border rounded-2xl border border-workspace-border">
                  {#each selectedAnswer.criterionResults as criterion (`${criterion.criterionIndex}:${criterion.pointsAwarded}:${criterion.rationale}`)}
                    <div class="flex items-start gap-3 px-4 py-2.5">
                      <span
                        class={`mt-1 inline-block size-2 shrink-0 rounded-full ${criterionDotClass(
                          criterion.pointsAwarded,
                          criterion.points
                        )}`}
                      ></span>
                      <div class="min-w-0 flex-1">
                        <div class="text-sm font-medium text-workspace-text-primary">
                          {criterion.label || `Criterion ${criterion.criterionIndex + 1}`}
                        </div>
                        {#if criterion.rationale}
                          <div class="mt-0.5 whitespace-pre-wrap text-xs text-workspace-text-muted">
                            {criterion.rationale}
                          </div>
                        {/if}
                      </div>
                      {#if onsavecriterionscore}
                        <div class="flex shrink-0 flex-wrap gap-0.5">
                          {#each Array.from({ length: criterion.points + 1 }, (_, i) => i) as pointValue (pointValue)}
                            <button
                              type="button"
                              class={`inline-flex h-6 w-6 items-center justify-center rounded-md border text-xs tabular-nums transition-colors ${
                                pointValue === criterion.pointsAwarded
                                  ? 'border-message-success-border bg-message-success-bg text-message-success-text'
                                  : 'border-border bg-surface-card-control text-workspace-text-muted hover:bg-muted/40'
                              } ${globalControlsDisabled || criterionSaving(selectedAnswer) ? 'pointer-events-none opacity-50' : ''}`}
                              title={`${pointValue} point${pointValue !== 1 ? 's' : ''}`}
                              disabled={globalControlsDisabled || criterionSaving(selectedAnswer)}
                              onclick={() => {
                                if (pointValue !== criterion.pointsAwarded && submission) {
                                  void onsavecriterionscore?.(
                                    submission.studentRef,
                                    selectedAnswer.questionId,
                                    criterion.criterionIndex,
                                    pointValue
                                  );
                                }
                              }}
                            >{pointValue}</button>
                          {/each}
                        </div>
                      {:else}
                        <div class="shrink-0 text-xs font-semibold tabular-nums text-workspace-text-muted">
                          {criterion.pointsAwarded}/{criterion.points}
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {:else if submission}
      <div class="mt-8 flex flex-col items-center gap-2 text-center">
        <div class="text-sm text-workspace-text-secondary">
          Question details will appear here once processing begins.
        </div>
      </div>
    {/if}
  </div>
</div>
