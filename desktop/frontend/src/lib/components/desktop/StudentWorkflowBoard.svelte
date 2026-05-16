<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { AlertCircleIcon, PlayIcon } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import type { StudentIntakeSummary, StudentWorkflowSubmission } from '$lib/types';
  import type { ProgressTone } from '$lib/components/desktop/student-workflow-helpers';
  import HorizontalProgressBar from '$lib/components/desktop/HorizontalProgressBar.svelte';
  import { DesktopButton, IconButton, StatusBadge } from './ui';

  export let courseCode: string;
  export let displayName: string;
  export let intakeComplete: number;
  export let processingCount: number;
  export let attentionCount: number;
  export let gradedCount: number;
  export let readyCount: number;
  export let canonicalReadyCount: number;
  export let busyActionLabel: string | null;
  export let recoveryAvailable = false;
  export let recoveryBusy = false;
  export let stopWorkflowBusy = false;
  export let attentionItems: StudentWorkflowSubmission[];
  export let canonicalReadyRows: {
    student: { studentRef: string };
    displayName: string;
    item: StudentIntakeSummary;
    workflowSubmission: StudentWorkflowSubmission | null;
    stageText: string;
    stageProgress: number;
    stageTone: ProgressTone;
    stageComplete: boolean;
    stageActive: boolean;
  }[];

  export let onSelectStudent: (studentRef: string) => void;
  export let onBeginWorkflow: (() => Promise<void>) | null;
  export let onStopWorkflow: (() => Promise<void>) | null = null;
  export let onRecoverWorkflow: (() => Promise<void>) | null = null;

  const attentionStages = new Set([
    'alignment_review',
    'detect_review',
    'parse_review',
    'manual_grading',
    'failed'
  ]);

  type WorkflowTableRow = {
    studentRef: string;
    displayName: string;
    stageText: string;
    stageDisplayText: string;
    stageProgress: number;
    stageTone: ProgressTone;
    stageComplete: boolean;
    stageActive: boolean;
    needsAttention: boolean;
  };

  $: workflowRows = canonicalReadyRows
    .map((entry): WorkflowTableRow => {
      const submission = entry.workflowSubmission;
      const needsAttention = submission ? attentionStages.has(submission.stage) : false;
      return {
        studentRef: entry.student.studentRef,
        displayName: entry.displayName,
        stageText: entry.stageText,
        stageDisplayText: compactStageLabel(entry.stageText),
        stageProgress: entry.stageProgress,
        stageTone: entry.stageTone,
        stageComplete: entry.stageComplete,
        stageActive: entry.stageActive,
        needsAttention
      };
    })
    .sort((a, b) => {
      if (a.needsAttention !== b.needsAttention) return a.needsAttention ? -1 : 1;
      return a.displayName.localeCompare(b.displayName);
    });

  function handleSelectStudent(studentRef: string) {
    onSelectStudent(studentRef);
  }

  function compactStageLabel(stageText: string): string {
    switch (stageText) {
      case 'waiting':
        return 'Waiting';
      case 'stopped':
        return 'Stopped';
      case 'aligning':
        return 'Aligning';
      case 'alignment review':
        return 'Alignment';
      case 'canonicalizing':
        return 'Canonicalizing';
      case 'transforming':
        return 'Transforming';
      case 'detecting':
        return 'Detecting';
      case 'region review':
        return 'Regions';
      case 'cropping':
        return 'Cropping';
      case 'screening PII':
        return 'Screening';
      case 'parsing':
        return 'Parsing';
      case 'parse review':
        return 'Parse';
      case 'grading':
        return 'Grading';
      case 'manual grading':
        return 'Manual';
      case 'draft grading ready':
        return 'Graded';
      case 'failed':
        return 'Failed';
      default:
        return stageText || 'Waiting';
    }
  }

  $: workflowStartDisabled =
    busyActionLabel !== null || recoveryAvailable || canonicalReadyRows.length === 0;

  function handleBeginWorkflow() {
    if (workflowStartDisabled) {
      return;
    }
    void onBeginWorkflow?.();
  }
</script>

<div class="flex items-start justify-between gap-4">
  <div class="min-w-0">
    <div class="text-base font-semibold text-workspace-text-primary">Exam Workflow</div>
    <div class="text-sm text-workspace-text-muted">
      {courseCode} · {displayName} · {canonicalReadyCount} submission{canonicalReadyCount === 1 ? '' : 's'} ready
    </div>
  </div>
  <div class="flex shrink-0 items-center gap-2">
    {#if recoveryAvailable}
      <DesktopButton
        variant="secondary"
        disabled={recoveryBusy || onRecoverWorkflow === null}
        onclick={() => void onRecoverWorkflow?.()}
      >
        {recoveryBusy ? 'Recovering…' : 'Recover Workflow'}
      </DesktopButton>
    {:else if busyActionLabel !== null}
      <DesktopButton
        variant="secondary"
        disabled={stopWorkflowBusy || onStopWorkflow === null}
        onclick={() => void onStopWorkflow?.()}
      >
        {stopWorkflowBusy ? 'Stopping…' : 'Stop Workflow'}
      </DesktopButton>
    {/if}
    <DesktopButton
      variant="secondary"
      disabled={busyActionLabel !== null || canonicalReadyRows.length === 0}
      onclick={handleBeginWorkflow}
    >
      {busyActionLabel ?? 'Begin Workflow'}
    </DesktopButton>
  </div>
</div>

{#if recoveryAvailable}
  <div
    class="mt-4 rounded-xl border border-message-warning-border bg-message-warning-bg px-4 py-3 text-sm text-message-warning-text"
    role="status"
  >
    This workflow is marked running, but the desktop runtime has no active job. Recover it to
    unlock review and restart controls.
  </div>
{/if}

<div class="mt-5 overflow-hidden rounded-3xl border border-workspace-border bg-surface-card-subtle">
  <div class="flex items-stretch">
    <div class="flex min-w-0 flex-1 items-center justify-center gap-3 bg-surface-card-control px-6 py-5">
      <div class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">Prepared</div>
      <div class="text-2xl font-semibold text-workspace-text-primary">{intakeComplete}</div>
    </div>
    <div class="w-px bg-workspace-border"></div>
    <div class="flex min-w-0 flex-1 items-center justify-center gap-3 px-6 py-5">
      <div class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">Processing</div>
      <div class="text-2xl font-semibold text-workspace-text-primary">{processingCount}</div>
    </div>
    <div class="w-px bg-workspace-border"></div>
    <div class="flex min-w-0 flex-1 items-center justify-center gap-3 px-6 py-5">
      <div class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">Needs review</div>
      <div class="text-2xl font-semibold text-workspace-text-primary">{attentionCount}</div>
    </div>
    <div class="w-px bg-workspace-border"></div>
    <div class="flex min-w-0 flex-1 items-center justify-center gap-3 px-6 py-5">
      <div class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">Graded</div>
      <div class="text-2xl font-semibold text-workspace-text-primary">{gradedCount}</div>
    </div>
  </div>

  <div class="border-t border-workspace-border px-6 py-5">
    <div class="flex flex-col items-center justify-center pb-4 text-center">
      <IconButton
        variant="ghost"
        size="default"
        class="size-16 rounded-full border-workspace-border bg-surface-card-control text-workspace-text-muted hover:bg-interaction-hover hover:text-workspace-text-primary disabled:opacity-45"
        ariaLabel="Begin student workflow"
        title="Begin student workflow"
        disabled={workflowStartDisabled}
        onclick={handleBeginWorkflow}
      >
        <HugeiconsIcon icon={PlayIcon} size={28} strokeWidth={1.8} aria-hidden="true" />
      </IconButton>
      <div class="mt-4 text-lg font-semibold text-workspace-text-primary">
        Ready to process {readyCount} exams
      </div>
      <div class="mt-2 max-w-[34rem] text-sm text-workspace-text-secondary">
        Aligns pages, finds answer areas, checks for private information, reads responses, and grades what it can. Anything needing your review stays visible here.
      </div>
    </div>

    <div class="mt-5 flex items-center justify-end gap-4">
      {#if attentionItems.length > 0}
        <StatusBadge tone="warning" class="gap-2">
          <HugeiconsIcon icon={AlertCircleIcon} size={14} strokeWidth={1.8} aria-hidden="true" />
          {attentionItems.length} need review
        </StatusBadge>
      {/if}
    </div>

    <div class="mt-3">
      {#if canonicalReadyRows.length === 0}
        <div class="py-2 text-sm text-workspace-text-secondary">
          No submissions are ready yet. Upload a student PDF to prepare it for grading.
        </div>
      {:else}
        <div class="max-h-[30rem] overflow-y-auto">
          <div class="grid grid-cols-[repeat(auto-fill,minmax(13rem,1fr))] gap-3">
            {#each workflowRows as row (row.studentRef)}
              <button
                type="button"
                class={`min-w-0 rounded-2xl border px-3 py-3 text-left text-sm transition-colors hover:bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                  row.needsAttention
                    ? 'border-message-warning-border bg-message-warning-bg'
                    : 'border-workspace-border bg-surface-card-control'
                }`}
                aria-label={row.needsAttention ? `Review ${row.displayName}` : `Open ${row.displayName} workflow`}
                onclick={() => handleSelectStudent(row.studentRef)}
              >
                <div class="flex min-w-0 items-start gap-2">
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium text-workspace-text-primary">{row.displayName}</div>
                  </div>
                  {#if row.needsAttention}
                    <span
                      class="inline-flex size-8 shrink-0 items-center justify-center rounded-xl border border-message-warning-border bg-surface-card-control text-message-warning-text"
                      title={`Review ${row.displayName}`}
                    >
                      <HugeiconsIcon icon={AlertCircleIcon} size={17} strokeWidth={1.8} aria-hidden="true" />
                    </span>
                  {/if}
                </div>
                <div class="mt-2">
                  <HorizontalProgressBar
                    label={row.stageDisplayText}
                    title={row.stageText}
                    progress={row.stageProgress}
                    tone={row.stageTone}
                    active={row.stageActive}
                    complete={row.stageComplete}
                    className="w-full"
                  />
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
