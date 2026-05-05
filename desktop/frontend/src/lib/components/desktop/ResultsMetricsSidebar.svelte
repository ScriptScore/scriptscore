<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type {
    LmsUploadAttemptResult,
    LmsUploadStudentResult,
    ResultsExamMetrics,
    ResultsLmsReviewSummary,
  } from '$lib/types';
  import type { ResultsScoreDisplayMode } from '$lib/stores/resultsWorkspaceView';
  import {
    clampPercent,
    displayNameForStudent,
    formatMetricScore,
    formatPercent,
    formatScoreDisplay,
  } from './results-workspace-helpers';
  import { DesktopButton, InlineMessage, StatusBadge, Surface, type FeedbackTone } from './ui';

  export let metrics: ResultsExamMetrics | null = null;
  export let reviewSummary: ResultsLmsReviewSummary | null = null;
  export let scoreDisplayMode: ResultsScoreDisplayMode = 'percent';
  export let examMaxPoints: number | null = null;
  export let studentDisplayNamesByRef: Record<string, string> = {};
  export let uploadAttempts: LmsUploadAttemptResult[] = [];
  export let lmsLinked = true;
  export let selectedAttemptId: string | null = null;
  export let retryableFailureCount = 0;
  export let busy = false;
  export let onSelectAttempt: ((attemptId: string | null) => void) | null = null;
  export let onRetryUpload: ((attemptId: string) => Promise<void>) | null = null;

  const UPLOAD_HISTORY_POPOVER_WIDTH_PX = 288;
  const UPLOAD_HISTORY_POPOVER_MIN_HEIGHT_PX = 180;
  const UPLOAD_HISTORY_POPOVER_MAX_HEIGHT_PX = 360;
  const UPLOAD_HISTORY_POPOVER_VIEWPORT_PADDING_PX = 16;
  const UPLOAD_HISTORY_POPOVER_GAP_PX = 8;

  let uploadHistoryPopoverRef: HTMLDivElement | null = null;
  let openAttemptId: string | null = null;
  let openAttemptPlacement: 'above' | 'below' = 'below';
  let openAttemptPopoverLeftPx = 0;
  let openAttemptPopoverTopPx = 0;
  let openAttemptPopoverMaxHeightPx = UPLOAD_HISTORY_POPOVER_MAX_HEIGHT_PX;
  let openAttemptTriggerElement: HTMLElement | null = null;

  function uploadAttemptLabel(attempt: LmsUploadAttemptResult): string {
    return `${attempt.mode === 'dry_run' ? 'Dry run' : 'Upload'} · ${attempt.successCount}/${attempt.attemptedCount}`;
  }

  function uploadHistoryBadgeTone(attempt: LmsUploadAttemptResult): FeedbackTone {
    return attempt.failureCount > 0 ? 'error' : 'success';
  }

  function metricPercent(value: number | null | undefined): number | null {
    if (examMaxPoints === null || examMaxPoints <= 0 || value === null || value === undefined) {
      return null;
    }
    return clampPercent((value / examMaxPoints) * 100);
  }

  function compactAttemptDate(timestamp: string): string {
    const parsed = Number(timestamp);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      return timestamp;
    }
    return new Intl.DateTimeFormat('en-US', {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      timeZone: 'UTC'
    }).format(new Date(parsed * 1000));
  }

  function attemptDurationLabel(attempt: LmsUploadAttemptResult): string {
    const started = Number(attempt.startedAt);
    const finished = Number(attempt.finishedAt);
    if (!Number.isFinite(started) || !Number.isFinite(finished) || finished < started) {
      return 'Duration unavailable';
    }
    const seconds = Math.max(0, Math.round(finished - started));
    if (seconds < 60) {
      return `${seconds}s`;
    }
    const minutes = Math.floor(seconds / 60);
    const remainderSeconds = seconds % 60;
    if (minutes < 60) {
      return remainderSeconds === 0 ? `${minutes}m` : `${minutes}m ${remainderSeconds}s`;
    }
    const hours = Math.floor(minutes / 60);
    const remainderMinutes = minutes % 60;
    return remainderMinutes === 0 ? `${hours}h` : `${hours}h ${remainderMinutes}m`;
  }

  function studentResultBadgeTone(result: LmsUploadStudentResult): FeedbackTone {
    if (result.status === 'uploaded') {
      return 'success';
    }
    if (result.status === 'failed') {
      return 'error';
    }
    return 'info';
  }

  function studentResultLabel(result: LmsUploadStudentResult): string {
    switch (result.status) {
      case 'uploaded':
        return 'Uploaded';
      case 'failed':
        return 'Failed';
      case 'ready':
      default:
        return 'Ready';
    }
  }

  function handleWindowPointerDown(event: PointerEvent) {
    const target = event.target;
    if (!(target instanceof Node)) {
      return;
    }
    if (openAttemptId && !uploadHistoryPopoverRef?.contains(target)) {
      clearOpenAttemptPopover();
    }
  }

  function handleWindowKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape' && openAttemptId) {
      event.preventDefault();
      clearOpenAttemptPopover();
    }
  }

  function handleWindowResize() {
    if (openAttemptId) {
      clearOpenAttemptPopover();
    }
  }

  function clearOpenAttemptPopover(restoreFocus = true) {
    openAttemptId = null;
    if (restoreFocus) {
      queueMicrotask(() => openAttemptTriggerElement?.focus());
    }
  }

  function toggleAttemptPopover(attemptId: string, target: HTMLElement) {
    onSelectAttempt?.(attemptId);
    if (openAttemptId === attemptId) {
      clearOpenAttemptPopover();
      return;
    }

    const rect = target.getBoundingClientRect();
    const viewportHeight = window.innerHeight;
    const viewportWidth = window.innerWidth;
    const availableBelow = Math.max(
      0,
      viewportHeight - rect.bottom - UPLOAD_HISTORY_POPOVER_VIEWPORT_PADDING_PX
    );
    const availableAbove = Math.max(
      0,
      rect.top - UPLOAD_HISTORY_POPOVER_VIEWPORT_PADDING_PX
    );
    openAttemptPlacement =
      availableBelow < UPLOAD_HISTORY_POPOVER_MIN_HEIGHT_PX && availableAbove > availableBelow
        ? 'above'
        : 'below';
    const availableSpace =
      openAttemptPlacement === 'above' ? availableAbove : availableBelow;
    openAttemptPopoverMaxHeightPx = Math.max(
      UPLOAD_HISTORY_POPOVER_MIN_HEIGHT_PX,
      Math.min(UPLOAD_HISTORY_POPOVER_MAX_HEIGHT_PX, availableSpace)
    );
    openAttemptPopoverLeftPx = Math.min(
      viewportWidth - UPLOAD_HISTORY_POPOVER_WIDTH_PX - UPLOAD_HISTORY_POPOVER_VIEWPORT_PADDING_PX,
      Math.max(
        UPLOAD_HISTORY_POPOVER_VIEWPORT_PADDING_PX,
        rect.right - UPLOAD_HISTORY_POPOVER_WIDTH_PX
      )
    );
    openAttemptPopoverTopPx =
      openAttemptPlacement === 'above'
        ? rect.top - UPLOAD_HISTORY_POPOVER_GAP_PX
        : rect.bottom + UPLOAD_HISTORY_POPOVER_GAP_PX;
    openAttemptTriggerElement = target;
    openAttemptId = attemptId;
  }
</script>

<svelte:window
  on:pointerdown={handleWindowPointerDown}
  on:keydown={handleWindowKeyDown}
  on:resize={handleWindowResize}
/>

<aside
  class="w-[20rem] shrink-0 overflow-y-auto bg-surface-sidebar px-4 py-4"
  role="region"
  aria-label="Results metrics and upload history"
>
  <div class="space-y-3">
    <section>
      <div class="shell-eyebrow text-workspace-text-muted">Exam metrics</div>
      <div class="mt-3 grid grid-cols-4 gap-3 text-center text-sm">
        <div class="min-w-0">
          <div class="text-workspace-text-muted">Avg</div>
          <div class="mt-1 font-semibold text-workspace-text-primary">
            {metrics
              ? formatScoreDisplay(
                  metrics.averageScore,
                  metricPercent(metrics.averageScore),
                  scoreDisplayMode
                )
              : 'N/A'}
          </div>
        </div>
        <div class="min-w-0">
          <div class="text-workspace-text-muted">Median</div>
          <div class="mt-1 font-semibold text-workspace-text-primary">
            {metrics
              ? formatScoreDisplay(
                  metrics.medianScore,
                  metricPercent(metrics.medianScore),
                  scoreDisplayMode
                )
              : 'N/A'}
          </div>
        </div>
        <div class="min-w-0">
          <div class="text-workspace-text-muted">Min</div>
          <div class="mt-1 font-semibold text-workspace-text-primary">
            {#if !metrics}
              N/A
            {:else if scoreDisplayMode === 'percent'}
              {formatScoreDisplay(metrics.minScore, metricPercent(metrics.minScore), 'percent')}
            {:else}
              {formatMetricScore(metrics.minScore, 1)}
            {/if}
          </div>
        </div>
        <div class="min-w-0">
          <div class="text-workspace-text-muted">Max</div>
          <div class="mt-1 font-semibold text-workspace-text-primary">
            {#if !metrics}
              N/A
            {:else if scoreDisplayMode === 'percent'}
              {formatScoreDisplay(metrics.maxScore, metricPercent(metrics.maxScore), 'percent')}
            {:else}
              {formatMetricScore(metrics.maxScore, 1)}
            {/if}
          </div>
        </div>
      </div>
    </section>

    {#if reviewSummary?.hasUnreviewedQuestions}
      <InlineMessage class="rounded-2xl" tone="warning">
        {reviewSummary.unreviewedQuestionCount} moderation question{reviewSummary.unreviewedQuestionCount === 1 ? '' : 's'} still need review.
      </InlineMessage>
    {/if}

    <Surface as="section" class="px-4 py-4" variant="cardControl" radius="2xl" shadow="soft">
      <div class="shell-section-title text-workspace-text-primary">Question difficulty</div>
      <div class="mt-3 space-y-2">
        {#if !metrics || metrics.questionMetrics.length === 0}
          <div class="text-sm text-workspace-text-secondary">No question metrics yet.</div>
        {:else}
          {#each metrics.questionMetrics as metric (metric.questionId)}
            <article class="rounded-2xl bg-workspace-empty px-3 py-3">
              <div class="flex items-center justify-between gap-2">
                <div class="min-w-0 text-sm font-semibold text-workspace-text-primary">
                  Q{metric.questionNumber} · Avg {formatScoreDisplay(metric.averagePoints, metric.averagePercent, scoreDisplayMode)}
                </div>
                <StatusBadge
                  class="shrink-0 px-2 py-0.5 text-[11px] font-semibold"
                  tone={metric.reviewed ? 'success' : 'warning'}
                >
                  {metric.reviewed ? 'Reviewed' : 'Needs review'}
                </StatusBadge>
              </div>
              <div class="mt-2 flex items-center gap-2 text-[11px] text-workspace-text-muted">
                <div class="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-surface-card-control">
                  <div
                    class="h-full rounded-full bg-message-warning-border"
                    style:width={`${clampPercent(metric.difficultyPercent)}%`}
                  ></div>
                </div>
                <span>{formatPercent(metric.difficultyPercent)}</span>
              </div>
            </article>
          {/each}
        {/if}
      </div>
    </Surface>

    {#if lmsLinked}
    <Surface as="section" class="px-4 py-4" variant="cardControl" radius="2xl" shadow="soft">
      <div class="flex items-start justify-between gap-3">
        <div>
          <div class="shell-section-title text-workspace-text-primary">Upload history</div>
          <div class="mt-1 text-sm text-workspace-text-secondary">Privacy-safe summaries only.</div>
        </div>
        <DesktopButton
          size="compact"
          variant="subtle"
          disabled={busy || !selectedAttemptId || retryableFailureCount === 0}
          onclick={() => selectedAttemptId && void onRetryUpload?.(selectedAttemptId)}
        >
          Retry failed
        </DesktopButton>
      </div>

      <div class="mt-3 space-y-2" bind:this={uploadHistoryPopoverRef}>
        {#if uploadAttempts.length === 0}
          <div class="text-sm text-workspace-text-secondary">No upload attempts yet.</div>
        {:else}
          {#each uploadAttempts.slice().reverse() as attempt (attempt.attemptId)}
            <div class="relative">
              <button
                type="button"
                class={`w-full rounded-2xl px-3 py-3 text-left transition-colors ${
                  attempt.attemptId === selectedAttemptId || attempt.attemptId === openAttemptId
                    ? 'bg-workspace-sidebar-active'
                    : 'bg-surface-message hover:bg-surface-card-hover'
                }`}
                onclick={(event) => toggleAttemptPopover(attempt.attemptId, event.currentTarget as HTMLElement)}
              >
                <div class="flex items-start justify-between gap-2">
                  <div>
                    <div class="text-sm font-semibold text-workspace-text-primary">
                      {uploadAttemptLabel(attempt)}
                    </div>
                    <div class="mt-1 text-xs text-workspace-text-secondary">
                      {compactAttemptDate(attempt.startedAt)} · {attemptDurationLabel(attempt)}
                    </div>
                  </div>
                  <StatusBadge
                    class="px-2 py-0.5 text-[11px] font-semibold"
                    tone={uploadHistoryBadgeTone(attempt)}
                  >
                    {attempt.failureCount > 0 ? 'Attention' : 'Clean'}
                  </StatusBadge>
                </div>
              </button>

              {#if attempt.attemptId === openAttemptId}
                <div
                  class={`fixed z-40 flex w-[18rem] flex-col overflow-hidden rounded-xl border border-border-default bg-surface-overlay p-3 shadow-[var(--surface-shadow-strong)] ${
                    openAttemptPlacement === 'above' ? '-translate-y-full' : ''
                  }`}
                  role="dialog"
                  aria-label="Upload attempt details"
                  style:left={`${openAttemptPopoverLeftPx}px`}
                  style:top={`${openAttemptPopoverTopPx}px`}
                  style:max-height={`${openAttemptPopoverMaxHeightPx}px`}
                >
                  <div class="flex items-start justify-between gap-3">
                    <div>
                      <div class="text-sm font-semibold text-workspace-text-primary">
                        {uploadAttemptLabel(attempt)}
                      </div>
                      <div class="mt-1 text-[11px] text-workspace-text-muted">
                        {compactAttemptDate(attempt.startedAt)} · {attemptDurationLabel(attempt)}
                      </div>
                    </div>
                    <StatusBadge
                      class="px-2 py-0.5 text-[11px] font-semibold"
                      tone={uploadHistoryBadgeTone(attempt)}
                    >
                      {attempt.failureCount > 0 ? 'Attention' : 'Clean'}
                    </StatusBadge>
                  </div>

                  <div class="mt-3 flex-1 space-y-2 overflow-y-auto pr-1">
                    {#if attempt.studentResults.length === 0}
                      <div class="text-sm text-workspace-text-secondary">No student details recorded.</div>
                    {:else}
                      {#each attempt.studentResults as result (result.studentRef)}
                        <article class="rounded-xl bg-workspace-empty px-3 py-2">
                          <div class="flex items-start justify-between gap-2">
                            <div class="min-w-0 text-sm font-medium text-workspace-text-primary">
                              {displayNameForStudent(result.studentRef, studentDisplayNamesByRef)}
                            </div>
                            <StatusBadge
                              class="shrink-0 px-2 py-0.5 text-[11px] font-semibold"
                              tone={studentResultBadgeTone(result)}
                            >
                              {studentResultLabel(result)}
                            </StatusBadge>
                          </div>
                          {#if result.sanitizedError}
                            <div class="mt-2 text-[11px] leading-5 text-message-error-text">
                              {result.sanitizedError}
                            </div>
                          {/if}
                        </article>
                      {/each}
                    {/if}
                  </div>
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </Surface>
    {/if}
  </div>
</aside>
