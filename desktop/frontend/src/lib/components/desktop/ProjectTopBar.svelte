<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount } from 'svelte';
  import AppMark from './AppMark.svelte';
  import NotificationToasts from './NotificationToasts.svelte';
  import { DesktopButton, DesktopPopover, StatusBadge, type FeedbackTone } from './ui';
  import type { WorkerActivity, WorkerStatus } from '$lib/types';

  export let appTitle = 'ScriptScore Desktop';
  export let workerStatus: WorkerStatus = 'starting';
  export let workerProgress: number | null = null;
  export let workerActivity: WorkerActivity | null = null;
  export let workflowStage = 'template_setup_not_started';
  export let workflowLabel = 'Template setup not started';
  export let onOpenActiveTraces: ((jobIds: string[]) => void) | null = null;

  let workerPopoverOpen = false;
  let nowMs = Date.now();

  $: activeJobs = workerActivity?.activeJobs ?? [];
  $: pendingJobCount = workerActivity?.pendingJobCount ?? 0;
  $: activeTraceJobIds = activeJobs.map((job) => job.jobId);
  $: primaryActiveJob = activeJobs[0] ?? null;
  $: canShowMore = onOpenActiveTraces !== null;
  $: workerBadgeLabel =
    workerStatus === 'busy' && workerProgress !== null ? `Busy ${workerProgress}%` : workerStatus;

  onMount(() => {
    const timer = window.setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    return () => {
      window.clearInterval(timer);
    };
  });

  function workerStatusTone(status: WorkerStatus): FeedbackTone {
    switch (status) {
      case 'ready':
        return 'success';
      case 'busy':
      case 'starting':
        return 'info';
      case 'error':
        return 'error';
      default:
        return 'muted';
    }
  }

  function workflowSubtitleClass(stage: string) {
    switch (stage) {
      case 'student_intake_ready':
        return 'text-message-success-text';
      case 'student_grading_complete':
      case 'results_finalized':
      case 'results_uploaded':
        return 'text-message-success-text';
      case 'template_setup_failed':
        return 'text-message-error-text';
      case 'template_setup_running':
      case 'redaction_review':
      case 'question_review':
      case 'rubric_authoring':
      case 'student_workflow_running':
      case 'student_grading':
      case 'results_upload_ready':
        return 'text-message-info-text';
      case 'student_workflow_review':
      case 'results_finalization_pending':
      case 'results_upload_attention':
        return 'text-message-warning-text';
      default:
        return 'text-text-secondary';
    }
  }

  function commandLabel(commandName: string): string {
    const labels: Record<string, string> = {
      'create_project': 'Create project',
      'regrade_question_answers': 'Regrade answers',
      'exam.analyze': 'Analyze exam',
      'exam.generate-rubric': 'Generate rubric',
      'grading.score-preliminary': 'Grade answers',
      'grading.generate-feedback': 'Generate feedback',
      'scans.detect': 'Detect answers',
      'scans.ocr': 'OCR scan',
      'scans.parse': 'Parse answers',
      'scans.pii': 'Redact scan',
      'results.export': 'Export results',
      'results.lms-upload': 'Upload results',
      'smoke.ping': 'Runtime check'
    };
    return labels[commandName] ?? commandName;
  }

  function timestampMs(value: string | null | undefined): number | null {
    if (!value) {
      return null;
    }
    const parsed = Date.parse(value);
    return Number.isFinite(parsed) ? parsed : null;
  }

  function formatStartedAt(value: string | null | undefined): string {
    const parsed = timestampMs(value);
    if (parsed === null) {
      return 'Unknown';
    }
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: 'medium',
      timeStyle: 'medium'
    }).format(new Date(parsed));
  }

  function formatDuration(value: string | null | undefined): string {
    const parsed = timestampMs(value);
    if (parsed === null) {
      return 'Unknown';
    }
    const totalSeconds = Math.max(0, Math.floor((nowMs - parsed) / 1000));
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;
    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`;
    }
    if (minutes > 0) {
      return `${minutes}m ${seconds}s`;
    }
    return `${seconds}s`;
  }

  function showMore() {
    if (!onOpenActiveTraces) {
      return;
    }
    onOpenActiveTraces(activeTraceJobIds);
    workerPopoverOpen = false;
  }
</script>

<header class="grid h-14 grid-cols-[minmax(0,1fr)_auto] items-center border-b border-border-default bg-surface-shell px-6">
  <div class="inline-flex min-w-0 items-center gap-3 justify-self-start text-left text-foreground">
    <AppMark size={32} alt="" className="size-8 shrink-0" />
    <div class="min-w-0 truncate text-lg font-medium leading-5 text-text-primary">
      <span>{appTitle}</span>
      <span class="px-2 text-text-muted" aria-hidden="true">-</span>
      <span class={`text-sm font-medium ${workflowSubtitleClass(workflowStage)}`}>
        {workflowLabel}
      </span>
    </div>
  </div>
  <div class="flex min-w-0 max-w-[50vw] items-center justify-self-end gap-3">
    <NotificationToasts placement="topbar" />
    <DesktopPopover
      open={workerPopoverOpen}
      onOpenChange={(open) => {
        workerPopoverOpen = open;
      }}
      triggerLabel={workerBadgeLabel}
      triggerClass="shrink-0 rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
      triggerAriaHaspopup="dialog"
      panelRole="dialog"
      panelAriaLabel="Worker activity"
      panelClass="w-72 p-3"
      align="end"
      title="Worker status. Click for activity and queue details."
    >
      <StatusBadge
        slot="trigger"
        tone={workerStatusTone(workerStatus)}
        class="h-9 shrink-0 px-3 font-semibold uppercase tracking-[0.16em]"
      >
        {workerBadgeLabel}
      </StatusBadge>
      <div class="grid gap-3 text-sm text-text-secondary">
        <div class="grid gap-2">
          <div>
            <div class="text-xs font-medium text-text-muted">Active</div>
            {#if primaryActiveJob}
              <div class="mt-0.5 truncate text-text-primary" title={primaryActiveJob.commandName}>
                {commandLabel(primaryActiveJob.commandName)}
              </div>
              <div class="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-1 text-xs">
                <div class="text-text-muted">Started</div>
                <div class="truncate text-text-primary" title={formatStartedAt(primaryActiveJob.startedAt)}>
                  {formatStartedAt(primaryActiveJob.startedAt)}
                </div>
                <div class="text-text-muted">Duration</div>
                <div class="text-text-primary">
                  {formatDuration(primaryActiveJob.startedAt)}
                </div>
              </div>
              {#if activeJobs.length > 1}
                <div class="mt-1 text-xs text-text-muted">
                  {activeJobs.length - 1} more running
                </div>
              {/if}
            {:else}
              <div class="mt-0.5 text-text-secondary">No active job</div>
            {/if}
          </div>
          <div>
            <div class="text-xs font-medium text-text-muted">Queue</div>
            <div class="mt-0.5 text-text-primary">
              {pendingJobCount} queued
            </div>
          </div>
        </div>
        <DesktopButton
          size="compact"
          fullWidth
          disabled={!canShowMore}
          title={activeTraceJobIds.length === 0 ? 'Show trace history.' : 'Show active job traces.'}
          data-popover-close
          onclick={showMore}
        >
          Show More
        </DesktopButton>
      </div>
    </DesktopPopover>
  </div>
</header>
