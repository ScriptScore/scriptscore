<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { toDesktopAssetUrl } from '$lib/desktop';
  import { Delete02Icon } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import type { StudentIntakeSummary, StudentWorkflowSubmission } from '$lib/types';
  import HorizontalProgressBar from './HorizontalProgressBar.svelte';
  import { stageLabel, stageProgressTone } from './student-workflow-helpers';
  import { DesktopButton, IconButton, PagePreviewFrame } from './ui';

  export let intakeItem: StudentIntakeSummary | null;
  export let submission: StudentWorkflowSubmission | null;
  export let displayName: string;
  export let stageProgress = 0;
  export let ondelete: (() => void) | null = null;
  export let deleteDisabled = false;
  export let onback: () => void;

  let pageNumber = 1;
  let prevStudentKey = '';

  $: examPaths =
    intakeItem?.examPagePaths?.filter(
      (p) => typeof p === 'string' && p.trim().length > 0
    ) ?? [];
  $: pageCount = examPaths.length;
  $: imageSrc =
    pageCount > 0 && pageNumber >= 1 && pageNumber <= pageCount
      ? toDesktopAssetUrl(examPaths[pageNumber - 1]!)
      : '';

  $: stageText = stageLabel(submission?.stage ?? 'intake_ready');
  $: stage = submission?.stage ?? 'intake_ready';
  $: stageTone = stageProgressTone(stage);
  $: stageActive = ['alignment', 'canonicalize', 'transform', 'detect', 'crop', 'parse', 'grading'].includes(stage);

  $: currentKey = `${intakeItem?.studentRef ?? ''}:${submission?.studentRef ?? ''}`;
  $: if (currentKey !== prevStudentKey) {
    pageNumber = 1;
    prevStudentKey = currentKey;
  }

</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="flex shrink-0 items-center justify-between gap-4 px-6 py-4">
    <DesktopButton class="whitespace-nowrap" variant="ghost" size="compact" onclick={onback}>Back to workflow</DesktopButton>
    <div class="min-w-0 text-center">
      <div class="truncate text-sm font-semibold text-workspace-text-primary">
        {displayName}
      </div>
    </div>
    <div class="flex items-center justify-end gap-2">
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

  <div class="flex min-h-0 flex-1 items-center justify-center px-6 pb-2">
    {#if pageCount > 0 && imageSrc}
      <PagePreviewFrame
        src={imageSrc}
        alt="Exam page {pageNumber}"
        class="max-h-[calc(100vh-11rem)] w-auto"
        imageClass="block max-h-[calc(100vh-11rem)] w-auto object-contain"
      />
    {:else}
      <div class="flex flex-col items-center gap-3 text-center">
        <div class="text-sm text-workspace-text-secondary">
          {intakeItem
            ? 'Scan ingest outputs (page PNGs) will appear here after processing.'
            : 'No submission on file for this student.'}
        </div>
      </div>
    {/if}
  </div>

  {#if pageCount > 0}
    <div class="flex shrink-0 items-center justify-center gap-3 px-6 pb-5">
      <DesktopButton
        variant="secondary"
        disabled={pageNumber <= 1}
        onclick={() => (pageNumber = Math.max(1, pageNumber - 1))}
      >
        ← Prev
      </DesktopButton>
      <div class="text-sm text-workspace-text-secondary">
        {pageNumber} / {pageCount}
      </div>
      <DesktopButton
        variant="secondary"
        disabled={pageNumber >= pageCount}
        onclick={() => (pageNumber = Math.min(pageCount, pageNumber + 1))}
      >
        Next →
      </DesktopButton>
    </div>
  {/if}
</div>
