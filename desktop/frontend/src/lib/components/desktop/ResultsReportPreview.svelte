<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { ResultStudentRow } from '$lib/types';
  import type {
    ResultsPreviewEntry,
    ResultsScoreDisplayMode,
    ResultsUploadProgressStatus,
  } from '$lib/stores/resultsWorkspaceView';
  import {
    aggregateScorePercent,
    formatPercentValue,
  } from './results-workspace-helpers';
  import { DesktopButton, InlineMessage, PagePreviewFrame, StatusBadge, type FeedbackTone } from './ui';

  export let selectedRow: ResultStudentRow | null = null;
  export let studentDisplayName = '';
  export let previewEntry: ResultsPreviewEntry | null = null;
  export let busy = false;
  export let scoreDisplayMode: ResultsScoreDisplayMode = 'percent';
  export let uploadProgressStatus: ResultsUploadProgressStatus | null = null;
  export let lmsLinked = true;
  export let onFinalize: ((studentRef: string) => Promise<void>) | null = null;
  export let onUnfinalize: ((studentRef: string) => Promise<void>) | null = null;

  function badgeTone(
    kind:
      | 'ready'
      | 'finalized'
      | 'stale'
      | 'validated'
      | 'uploading'
      | 'uploaded'
      | 'failed'
      | 'blocked'
  ): FeedbackTone {
    switch (kind) {
      case 'ready':
      case 'validated':
      case 'uploading':
        return 'info';
      case 'finalized':
      case 'uploaded':
        return 'success';
      case 'failed':
        return 'error';
      case 'stale':
      case 'blocked':
        return 'warning';
    }
  }

  function rowBadges(row: ResultStudentRow): string[] {
    const badges: string[] = [];
    if (uploadProgressStatus === 'ready') {
      badges.push('Validated');
    } else if (uploadProgressStatus === 'uploading') {
      badges.push('Uploading');
    } else if (uploadProgressStatus === 'uploaded') {
      badges.push('Uploaded');
    } else if (uploadProgressStatus === 'failed') {
      badges.push('Upload failed');
    }
    if (!row.readyToFinalize) {
      badges.push('Blocked');
    } else if (!row.finalized) {
      badges.push('Ready');
    }
    if (row.finalized) {
      badges.push('Finalized');
    }
    if (row.staleFinalization) {
      badges.push('Stale');
    }
    if (row.uploaded && uploadProgressStatus !== 'uploaded') {
      badges.push('Uploaded');
    }
    if (row.uploadFailed && uploadProgressStatus !== 'failed') {
      badges.push('Upload failed');
    }
    return badges;
  }

  function scoreLabel(row: ResultStudentRow): string {
    if (!row.aggregateComplete) {
      return 'Pending';
    }
    return scoreDisplayMode === 'percent'
      ? formatPercentValue(aggregateScorePercent(row), 1)
      : row.aggregateTotal.toString();
  }
</script>

<section class="flex min-h-0 flex-1 flex-col bg-surface-panel px-4 pt-4 pb-4" aria-label="Result report preview">
  <div class="mx-auto flex h-full w-full max-w-5xl flex-col gap-4">
    <header class="space-y-2">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 class="shell-title text-workspace-text-primary">
            {selectedRow ? studentDisplayName : 'Select a student'}
          </h1>
          {#if !selectedRow}
            <p class="shell-body text-workspace-text-secondary">
              The selected student’s uploaded HTML report will render here.
            </p>
          {/if}
        </div>
        {#if selectedRow}
          <div class="flex flex-wrap items-center justify-end gap-2">
            <span class="text-sm font-semibold text-workspace-text-primary">{scoreLabel(selectedRow)}</span>
            {#each rowBadges(selectedRow) as badge (badge)}
              <StatusBadge
                class="px-3 py-1 font-semibold uppercase tracking-[0.12em]"
                tone={badge === 'Ready'
                  ? badgeTone('ready')
                  : badge === 'Validated'
                    ? badgeTone('validated')
                    : badge === 'Uploading'
                      ? badgeTone('uploading')
                      : badge === 'Finalized'
                        ? badgeTone('finalized')
                        : badge === 'Stale'
                          ? badgeTone('stale')
                          : badge === 'Uploaded'
                            ? badgeTone('uploaded')
                            : badge === 'Upload failed'
                              ? badgeTone('failed')
                              : badgeTone('blocked')}
              >
                {badge}
              </StatusBadge>
            {/each}
          </div>
        {/if}
      </div>
    </header>

    {#if selectedRow?.blockedReasons.length}
      <InlineMessage class="rounded-2xl" tone="warning">
        {selectedRow.blockedReasons[0]}
      </InlineMessage>
    {/if}

    {#if selectedRow?.latestUploadError}
      <InlineMessage class="rounded-2xl" tone="error">
        {selectedRow.latestUploadError}
      </InlineMessage>
    {/if}

    <PagePreviewFrame class="min-h-0 flex-1">
      {#if !selectedRow}
        <div class="flex h-full items-center justify-center px-6 text-center text-sm text-workspace-text-secondary">
          Choose a student from the left sidebar to inspect the exact HTML report that will be uploaded.
        </div>
      {:else if previewEntry?.status === 'error'}
        <div class="flex h-full items-center justify-center px-6 text-center text-sm text-message-error-text">
          {previewEntry.error ?? 'The report preview could not be loaded.'}
        </div>
      {:else if previewEntry?.status !== 'ready'}
        <div class="flex h-full items-center justify-center px-6 text-center text-sm text-workspace-text-secondary">
          Loading uploaded report preview…
        </div>
      {:else}
        <iframe
          class="h-full w-full bg-[var(--workspace-page-bg)]"
          title={`${studentDisplayName} uploaded report preview`}
          sandbox=""
          srcdoc={previewEntry.html ?? ''}
        ></iframe>
      {/if}
    </PagePreviewFrame>

    {#if lmsLinked}
      <div class="flex flex-wrap gap-3">
        <DesktopButton
          variant="primary"
          disabled={busy || !selectedRow || !selectedRow.readyToFinalize || selectedRow.finalized}
          onclick={() => selectedRow && void onFinalize?.(selectedRow.studentRef)}
        >
          Finalize
        </DesktopButton>
        <DesktopButton
          disabled={busy || !selectedRow || !selectedRow.finalized}
          onclick={() => selectedRow && void onUnfinalize?.(selectedRow.studentRef)}
        >
          Unfinalize
        </DesktopButton>
      </div>
    {/if}
  </div>
</section>
