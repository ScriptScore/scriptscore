<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { ProgressTone } from './student-workflow-helpers';

  export let label = '';
  export let progress: number | null = null;
  export let active = false;
  export let complete = false;
  export let tone: ProgressTone = 'info';
  export let className = '';
  export let showPercent = true;
  export let title: string | null = null;

  $: percent = clampPercent(progress);
  $: displayPercent = `${percent}%`;
  $: fillStyle = `width:${percent}%;`;
  $: toneClass = progressToneClass(tone, active, complete);
  $: fillClass = progressFillClass(tone, complete);

  function clampPercent(value: number | null): number {
    if (typeof value !== 'number' || Number.isNaN(value)) {
      return 0;
    }
    return Math.round(Math.max(0, Math.min(100, value)));
  }

  function progressToneClass(nextTone: ProgressTone, isActive: boolean, isComplete: boolean): string {
    if (isComplete || nextTone === 'success') {
      return 'border-message-success-border bg-message-success-bg text-message-success-text';
    }
    if (nextTone === 'warning') {
      return 'border-message-warning-border bg-message-warning-bg text-message-warning-text';
    }
    if (nextTone === 'error') {
      return 'border-message-error-border bg-message-error-bg text-message-error-text';
    }
    if (nextTone === 'muted') {
      return 'border-workspace-border bg-surface-card-control text-workspace-text-secondary';
    }
    return isActive
      ? 'border-message-info-border bg-message-info-bg text-message-info-text'
      : 'border-workspace-border bg-surface-card-control text-workspace-text-secondary';
  }

  function progressFillClass(nextTone: ProgressTone, isComplete: boolean): string {
    if (isComplete || nextTone === 'success') {
      return 'bg-message-success-border/70';
    }
    if (nextTone === 'warning') {
      return 'bg-message-warning-border/70';
    }
    if (nextTone === 'error') {
      return 'bg-message-error-border/70';
    }
    if (nextTone === 'muted') {
      return 'bg-workspace-border';
    }
    return 'bg-message-info-border/70';
  }
</script>

<div
  class={`relative h-10 overflow-hidden rounded-lg border ${toneClass} ${className}`}
  role="progressbar"
  aria-label={label}
  title={title ?? label}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={percent}
>
  <div
    class={`absolute inset-y-0 left-0 rounded-r-lg transition-[width] duration-500 ease-out ${fillClass}`}
    class:animate-pulse={active && !complete}
    style={fillStyle}
  ></div>
  <div
    class="absolute inset-0 bg-gradient-to-r from-transparent via-[var(--primary)]/20 to-transparent opacity-70"
    class:animate-pulse={active && !complete}
  ></div>
  <div
    class={`relative z-10 flex h-full items-center gap-3 px-3 text-xs font-semibold ${
      showPercent ? 'justify-between' : 'justify-center'
    }`}
  >
    <span class={`min-w-0 truncate ${showPercent ? '' : 'text-center'}`}>{label}</span>
    {#if showPercent}
      <span class="shrink-0 tabular-nums">{displayPercent}</span>
    {/if}
  </div>
</div>
